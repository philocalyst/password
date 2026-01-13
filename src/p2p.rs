use std::sync::Arc;

use anyhow::Result;
use iroh::{protocol::Router, Endpoint};
use iroh_blobs::{store::mem::MemStore, ticket::BlobTicket, BlobsProtocol};
use tokio::sync::Mutex;

// Represents the current sync state
#[derive(Debug, Clone)]
pub enum SyncState {
    // Not syncing
    Idle,
    // Sharing passwords, waiting for receiver
    Sharing { ticket: String },
    // Awaiting ticket input for receiving
    ReceiveInput { input: String },
    // Currently receiving passwords
    Receiving,
    // Sync completed successfully
    Completed { message: String },
    // Sync failed with error
    Error { message: String },
}

impl Default for SyncState {
    fn default() -> Self {
        Self::Idle
    }
}

// P2P sync handler for password store synchronization
pub struct P2PSync {
    endpoint: Endpoint,
    store: MemStore,
    blobs: BlobsProtocol,
    router: Option<Router>,
}

impl P2PSync {
    // Create a new P2P sync instance
    pub async fn new() -> Result<Self> {
        let endpoint = Endpoint::bind().await?;
        let store = MemStore::new();
        let blobs = BlobsProtocol::new(&store, None);

        Ok(Self {
            endpoint,
            store,
            blobs,
            router: None,
        })
    }

    // Share password data and return a ticket string
    // The data should be serialized password store bytes
    pub async fn share_data(&mut self, data: Vec<u8>) -> Result<String> {
        // Import the data as a blob
        let tag = self.blobs.add_bytes(data).await?;

        // Create ticket with our node info
        let node_id = self.endpoint.id();
        let ticket = BlobTicket::new(node_id.into(), tag.hash, tag.format);

        // Start the router to accept incoming connections
        let router = Router::builder(self.endpoint.clone())
            .accept(iroh_blobs::ALPN, self.blobs.clone())
            .spawn();

        self.router = Some(router);

        Ok(ticket.to_string())
    }

    // Receive password data from a ticket string
    // Returns the raw bytes of the password store
    pub async fn receive_data(&self, ticket_str: &str) -> Result<Vec<u8>> {
        let ticket: BlobTicket = ticket_str.parse()?;

        // Create downloader and fetch the blob
        let downloader = self.store.downloader(&self.endpoint);
        downloader
            .download(ticket.hash(), Some(ticket.addr().id))
            .await?;

        // Read the blob data using get_bytes
        let data = self.blobs.get_bytes(ticket.hash()).await?;

        Ok(data.to_vec())
    }

    // Gracefully shutdown the P2P sync
    pub async fn shutdown(self) -> Result<()> {
        if let Some(router) = self.router {
            router.shutdown().await?;
        } else {
            self.endpoint.close().await;
        }
        Ok(())
    }
}

// Async-safe wrapper for P2P sync operations
pub struct P2PSyncHandle {
    inner: Arc<Mutex<Option<P2PSync>>>,
}

impl P2PSyncHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    // Initialize a new P2P sync session
    pub async fn init(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        *guard = Some(P2PSync::new().await?);
        Ok(())
    }

    // Share data and return ticket
    pub async fn share(&self, data: Vec<u8>) -> Result<String> {
        let mut guard = self.inner.lock().await;
        if let Some(ref mut sync) = *guard {
            sync.share_data(data).await
        } else {
            anyhow::bail!("P2P sync not initialized")
        }
    }

    // Receive data from ticket
    pub async fn receive(&self, ticket: &str) -> Result<Vec<u8>> {
        let mut guard = self.inner.lock().await;
        if guard.is_none() {
            *guard = Some(P2PSync::new().await?);
        }
        let sync = guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("P2P sync not initialized"))?;
        sync.receive_data(ticket).await
    }

    // Shutdown and cleanup
    pub async fn shutdown(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        if let Some(sync) = guard.take() {
            sync.shutdown().await?;
        }
        Ok(())
    }

    // Check if currently active
    pub async fn is_active(&self) -> bool {
        self.inner.lock().await.is_some()
    }
}

impl Default for P2PSyncHandle {
    fn default() -> Self {
        Self::new()
    }
}