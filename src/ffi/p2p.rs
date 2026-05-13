use std::sync::Arc;

use super::{error::FfiError, store::PwdStore};
use crate::{p2p::{IrohSyncHandle, decode_store, encode_store}, store::{ShareTicket, StoreBackend}};

/// Synchronous handle to the Iroh P2P stack.
///
/// Embeds its own Tokio runtime so every call blocks until completion.
/// Only one `P2PHandle` should be active per process.
#[derive(uniffi::Object)]
pub struct P2PHandle {
	inner: IrohSyncHandle,
	rt:    tokio::runtime::Runtime,
}

#[uniffi::export]
impl P2PHandle {
	#[uniffi::constructor]
	pub fn new() -> Arc<Self> {
		let rt = tokio::runtime::Builder::new_multi_thread()
			.enable_all()
			.build()
			.expect("failed to build Tokio runtime");
		Arc::new(Self { inner: IrohSyncHandle::new(), rt })
	}

	/// Serialise and publish the store; returns an Iroh ticket string.
	pub fn share_store(&self, store: Arc<PwdStore>) -> Result<String, FfiError> {
		let payload = {
			let inner = store.inner.lock().unwrap();
			let loaded = inner.load(&store.branch).map_err(FfiError::from)?;
			encode_store(&loaded).map_err(FfiError::from)?
		};
		let ticket = self.rt.block_on(self.inner.share(payload)).map_err(FfiError::from)?;
		Ok(ticket.to_string())
	}

	/// Download the store from `ticket` and merge it into `target_store`.
	pub fn receive_into(&self, ticket: String, target_store: Arc<PwdStore>) -> Result<u64, FfiError> {
		let share_ticket = ShareTicket(ticket);
		let payload = self.rt.block_on(self.inner.receive(&share_ticket)).map_err(FfiError::from)?;
		let received = decode_store(payload).map_err(FfiError::from)?;
		let count = received.items.len() as u64;
		{
			let inner = target_store.inner.lock().unwrap();
			let mut current = inner.load(&target_store.branch).map_err(FfiError::from)?;
			for (name, item) in received.items {
				current.items.insert(name, item);
			}
			inner.save(&target_store.branch, &current).map_err(FfiError::from)?;
		}
		Ok(count)
	}

	/// Shut down the Iroh node and release the endpoint.
	pub fn shutdown(&self) -> Result<(), FfiError> {
		self.rt.block_on(self.inner.shutdown()).map_err(FfiError::from)
	}

	/// Returns `true` if the Iroh node is running.
	pub fn is_active(&self) -> bool { self.rt.block_on(self.inner.is_active()) }
}
