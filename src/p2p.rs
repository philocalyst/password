use std::sync::Arc;

use anyhow::Result;
use futures_lite::StreamExt;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use iroh_blobs::{ALPN as BLOBS_ALPN, BlobsProtocol, store::mem::MemStore};
use iroh_docs::{
	ALPN as DOCS_ALPN, DocTicket,
	api::{
		Doc,
		protocol::{AddrInfoOptions, ShareMode},
	},
	engine::LiveEvent,
	protocol::Docs,
	store::Query,
};
use iroh_gossip::{ALPN as GOSSIP_ALPN, net::Gossip};
use tokio::sync::Mutex;

// The fixed key under which the password store payload is stored.
const PASSWORDS_KEY: &[u8] = b"passwords";

// Represents the current sync state.
#[derive(Debug, Clone)]
pub enum SyncState {
	/// Not syncing.
	Idle,
	/// Sharing passwords; ticket can be handed to the other peer.
	Sharing { ticket: String },
	/// Awaiting ticket input for receiving.
	ReceiveInput { input: String },
	/// Currently receiving / syncing passwords.
	Receiving,
	/// Sync completed successfully.
	Completed { message: String },
	/// Sync failed.
	Error { message: String },
}

impl Default for SyncState {
	fn default() -> Self {
		Self::Idle
	}
}

/// All live handles needed to keep the node running.
struct NodeHandles {
	router: Router,
	/// Retained so we can read blob content after sync.
	blobs: MemStore,
	// Docs derefs to DocsApi, so all API calls go through this.
	docs: Docs,
}

/// P2P sync handler backed by iroh-docs.
///
/// The sender creates a document, writes the password payload as an entry,
/// and hands off a `DocTicket` to the receiver. The receiver imports the
/// ticket and waits for the entry via the live-sync event stream.
pub struct P2PSync {
	handles: Option<NodeHandles>,
}

impl P2PSync {
	/// Spin up an iroh endpoint with the full docs / blobs / gossip stack.
	pub async fn new() -> Result<Self> {
		let endpoint = Endpoint::builder(presets::N0).bind().await?;

		let blobs = MemStore::default();
		let gossip = Gossip::builder().spawn(endpoint.clone());
		let docs = Docs::memory().spawn(endpoint.clone(), (*blobs).clone(), gossip.clone()).await?;

		let router = Router::builder(endpoint)
			.accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
			.accept(GOSSIP_ALPN, gossip)
			.accept(DOCS_ALPN, docs.clone())
			.spawn();

		Ok(Self { handles: Some(NodeHandles { router, blobs, docs }) })
	}

	fn handles(&self) -> Result<&NodeHandles> {
		self.handles.as_ref().ok_or_else(|| anyhow::anyhow!("node not running"))
	}

	fn docs(&self) -> Result<&Docs> {
		Ok(&self.handles()?.docs)
	}

	/// Write `data` into a new document and return a `DocTicket` string.
	///
	/// The ticket encodes a read-only namespace capability plus this node's
	/// address, so the receiver can connect and pull the entry automatically.
	pub async fn share_data(&self, data: Vec<u8>) -> Result<String> {
		let api = self.docs()?;

		// Create a fresh document (new NamespaceSecret).
		let doc: Doc = api.create().await?;

		// Use this node's default author.
		let author = api.author_default().await?;

		// Write the payload as a single named entry.
		doc.set_bytes(author, PASSWORDS_KEY.to_vec(), data).await?;

		// Share as read-only so the receiver cannot write back.
		let ticket: DocTicket = doc.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;

		Ok(ticket.to_string())
	}

	/// Import a `DocTicket` string, join the document, wait for the entry to
	/// arrive, then return the raw bytes.
	pub async fn receive_data(&self, ticket_str: &str) -> Result<Vec<u8>> {
		let ticket: DocTicket = ticket_str.parse()?;
		let handles = self.handles()?;
		let api = &handles.docs;

		// Import the ticket and get a live-event stream.
		let (doc, mut events) = api.import_and_subscribe(ticket).await?;

		// Block until we see our entry arrive (remote insert) or discover it
		// was already present locally (local insert after re-import).
		loop {
			match events.next().await {
				Some(Ok(LiveEvent::InsertRemote { entry, .. })) => {
					if entry.key() == PASSWORDS_KEY {
						break;
					}
				}
				Some(Ok(LiveEvent::InsertLocal { entry })) => {
					if entry.key() == PASSWORDS_KEY {
						break;
					}
				}
				Some(Ok(_)) => continue,
				Some(Err(e)) => return Err(anyhow::anyhow!(e)),
				None => anyhow::bail!("event stream ended before entry arrived"),
			}
		}

		// Read the entry back and return its content bytes.
		let entry = doc
			.get_one(Query::single_latest_per_key().key_exact(PASSWORDS_KEY))
			.await?
			.ok_or_else(|| anyhow::anyhow!("entry missing after sync"))?;

		let content = handles.blobs.get_bytes(entry.content_hash()).await?;
		Ok(content.to_vec())
	}

	/// Gracefully shut down the router and close the endpoint.
	pub async fn shutdown(mut self) -> Result<()> {
		if let Some(handles) = self.handles.take() {
			handles.router.shutdown().await?;
		}
		Ok(())
	}
}

/// Async-safe wrapper for `P2PSync`.
pub struct P2PSyncHandle {
	inner: Arc<Mutex<Option<P2PSync>>>,
}

impl P2PSyncHandle {
	pub fn new() -> Self {
		Self { inner: Arc::new(Mutex::new(None)) }
	}

	/// Initialise a new P2P session (boots the full iroh stack).
	pub async fn init(&self) -> Result<()> {
		let mut guard = self.inner.lock().await;
		*guard = Some(P2PSync::new().await?);
		Ok(())
	}

	/// Share serialised password store bytes; returns a `DocTicket` string.
	pub async fn share(&self, data: Vec<u8>) -> Result<String> {
		let guard = self.inner.lock().await;
		match guard.as_ref() {
			Some(sync) => sync.share_data(data).await,
			None => anyhow::bail!("P2P sync not initialised"),
		}
	}

	/// Receive password store bytes from a `DocTicket` string.
	pub async fn receive(&self, ticket: &str) -> Result<Vec<u8>> {
		let mut guard = self.inner.lock().await;
		if guard.is_none() {
			*guard = Some(P2PSync::new().await?);
		}
		guard
			.as_ref()
			.ok_or_else(|| anyhow::anyhow!("P2P sync not initialised"))?
			.receive_data(ticket)
			.await
	}

	/// Shut down and clean up.
	pub async fn shutdown(&self) -> Result<()> {
		let mut guard = self.inner.lock().await;
		if let Some(sync) = guard.take() {
			sync.shutdown().await?;
		}
		Ok(())
	}

	/// Returns `true` if a session is currently active.
	pub async fn is_active(&self) -> bool {
		self.inner.lock().await.is_some()
	}
}

impl Default for P2PSyncHandle {
	fn default() -> Self {
		Self::new()
	}
}
