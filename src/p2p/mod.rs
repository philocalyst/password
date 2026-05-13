//! Iroh-backed P2P sync for the credential store.
//!
//! Concrete types, no trait abstraction. Two `IrohSync` instances can be
//! connected in-process for integration tests — spin up both from
//! `IrohSync::new()` and call `share_payload` / `receive_payload` directly.

use std::sync::Arc;

use anyhow::Result as AResult;
use futures_lite::StreamExt;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use iroh_blobs::{ALPN as BLOBS_ALPN, BlobsProtocol, store::mem::MemStore};
use iroh_docs::{ALPN as DOCS_ALPN, DocTicket, api::{Doc, protocol::{AddrInfoOptions, ShareMode}}, engine::LiveEvent, protocol::Docs, store::Query};
use iroh_gossip::{ALPN as GOSSIP_ALPN, net::Gossip};
use tokio::sync::Mutex;

use crate::{Error as PwdError, Result as PwdResult, store::{ShareTicket, StorePayload}};

/// The document key under which the full store payload is stored.
const PAYLOAD_KEY: &[u8] = b"store_payload";

struct NodeHandles {
	router: Router,
	blobs:  MemStore,
	docs:   Docs,
}

pub struct IrohSync {
	handles: Option<NodeHandles>,
}

impl IrohSync {
	/// Spin up the full iroh stack (docs / blobs / gossip) and return a
	/// ready-to-use `IrohSync` instance.
	pub async fn new() -> AResult<Self> {
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

	fn handles(&self) -> AResult<&NodeHandles> {
		self.handles.as_ref().ok_or_else(|| anyhow::anyhow!("iroh node not running"))
	}

	fn docs(&self) -> AResult<&Docs> { Ok(&self.handles()?.docs) }

	// ── share ─────────────────────────────────────────────────────────────────

	/// Publish `payload` into a new iroh-docs document and return a
	/// [`ShareTicket`] that the receiver can use to import it.
	pub async fn share_payload(&self, payload: StorePayload) -> PwdResult<ShareTicket> {
		let api = self.docs().map_err(PwdError::Iroh)?;
		let doc: Doc = api.create().await.map_err(PwdError::Iroh)?;

		let author = api.author_default().await.map_err(PwdError::Iroh)?;
		doc
			.set_bytes(author, PAYLOAD_KEY.to_vec(), payload.into_inner())
			.await
			.map_err(PwdError::Iroh)?;

		let ticket: DocTicket = doc
			.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses)
			.await
			.map_err(PwdError::Iroh)?;

		Ok(ShareTicket(ticket.to_string()))
	}

	// ── receive ───────────────────────────────────────────────────────────────

	/// Import `ticket`, wait for the payload entry to arrive, and return the
	/// raw [`StorePayload`] bytes.
	pub async fn receive_payload(&self, ticket: &ShareTicket) -> PwdResult<StorePayload> {
		let raw: DocTicket =
			ticket.as_str().parse::<DocTicket>().map_err(|e| PwdError::InvalidTicket(e.to_string()))?;

		let handles = self.handles().map_err(PwdError::Iroh)?;
		let (doc, mut events) = handles.docs.import_and_subscribe(raw).await.map_err(PwdError::Iroh)?;

		let mut pending_hash = None;
		loop {
			match events.next().await {
				Some(Ok(LiveEvent::InsertRemote { entry, .. }))
				| Some(Ok(LiveEvent::InsertLocal { entry }))
					if entry.key() == PAYLOAD_KEY =>
				{
					let hash = entry.content_hash();
					pending_hash = Some(hash);
					// Fallback: If it's already available for some reason, break immediately.
					if handles.blobs.get_bytes(hash).await.is_ok() {
						break;
					}
				}
				Some(Ok(LiveEvent::ContentReady { hash })) => {
					if Some(hash) == pending_hash {
						break;
					}
				}
				Some(Ok(_)) => continue,
				Some(Err(e)) => return Err(PwdError::Iroh(anyhow::anyhow!(e))),
				None => return Err(PwdError::PeerDisconnected),
			}
		}

		let entry = doc
			.get_one(Query::single_latest_per_key().key_exact(PAYLOAD_KEY))
			.await
			.map_err(PwdError::Iroh)?
			.ok_or_else(|| PwdError::Iroh(anyhow::anyhow!("entry missing after sync")))?;

		let bytes =
			handles.blobs.get_bytes(entry.content_hash()).await.map_err(|e| PwdError::Iroh(e.into()))?;

		Ok(StorePayload(bytes.to_vec()))
	}

	/// Gracefully shut down the router and release the endpoint.
	pub async fn shutdown(mut self) -> PwdResult<()> {
		if let Some(handles) = self.handles.take() {
			handles.router.shutdown().await.map_err(|e| PwdError::Iroh(e.into()))?;
		}
		Ok(())
	}
}

#[derive(Clone, Default)]
pub struct IrohSyncHandle {
	inner: Arc<Mutex<Option<IrohSync>>>,
}

impl IrohSyncHandle {
	pub fn new() -> Self { Self { inner: Arc::new(Mutex::new(None)) } }

	/// Boot the iroh node (idempotent — harmless to call multiple times).
	pub async fn init(&self) -> PwdResult<()> {
		let mut guard = self.inner.lock().await;
		if guard.is_none() {
			*guard = Some(IrohSync::new().await.map_err(PwdError::Iroh)?);
		}
		Ok(())
	}

	/// Publish `payload`; returns a [`ShareTicket`].
	pub async fn share(&self, payload: StorePayload) -> PwdResult<ShareTicket> {
		self.init().await?;
		let mut guard = self.inner.lock().await;
		guard.as_mut().unwrap().share_payload(payload).await
	}

	/// Import from `ticket`; returns the raw [`StorePayload`].
	pub async fn receive(&self, ticket: &ShareTicket) -> PwdResult<StorePayload> {
		self.init().await?;
		let mut guard = self.inner.lock().await;
		guard.as_mut().unwrap().receive_payload(ticket).await
	}

	/// Shut down and clean up.
	pub async fn shutdown(&self) -> PwdResult<()> {
		let mut guard = self.inner.lock().await;
		if let Some(sync) = guard.take() {
			sync.shutdown().await?;
		}
		Ok(())
	}

	/// Returns `true` when the iroh node is currently running.
	pub async fn is_active(&self) -> bool { self.inner.lock().await.is_some() }
}

// ── payload helpers
// ───────────────────────────────────────────────────────────

/// Serialise a [`crate::models::PasswordStore`] into a [`StorePayload`].
pub fn encode_store(store: &crate::models::PasswordStore) -> PwdResult<StorePayload> {
	let bytes = toml::to_string(store)?.into_bytes();
	Ok(StorePayload(bytes))
}

/// Deserialise a [`StorePayload`] back into a [`crate::models::PasswordStore`].
pub fn decode_store(payload: StorePayload) -> PwdResult<crate::models::PasswordStore> {
	let text = std::str::from_utf8(&payload.0)?;
	Ok(toml::from_str(text)?)
}
