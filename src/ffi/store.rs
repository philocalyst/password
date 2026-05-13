use std::{path::PathBuf, sync::Arc};

use pijul_at_core::Base32;

use super::{error::FfiError, types::{FfiChangeEntry, FfiItem}};
use crate::{models::AccountName, store::{DiffResult, StoreBackend, VersionedEntry}, versioning::PijulStore};

/// Thread-safe handle to a Pijul-backed credential store on a single branch.
#[derive(uniffi::Object)]
pub struct PwdStore {
	pub(super) inner:  std::sync::Mutex<PijulStore>,
	pub(super) branch: String,
}

#[uniffi::export]
impl PwdStore {
	/// Open or create a store at `store_dir` on `branch` (defaults to "main").
	#[uniffi::constructor]
	pub fn open(store_dir: String, branch: String) -> Result<Arc<Self>, FfiError> {
		let branch = if branch.is_empty() { "main".into() } else { branch };
		let store = PijulStore::open(PathBuf::from(&store_dir)).map_err(FfiError::from)?;
		store.init(&branch).map_err(FfiError::from)?;
		Ok(Arc::new(Self { inner: std::sync::Mutex::new(store), branch }))
	}

	pub fn branch(&self) -> String { self.branch.clone() }

	pub fn store_dir(&self) -> String {
		self.inner.lock().unwrap().store_dir.to_string_lossy().into_owned()
	}

	// ── read ──────────────────────────────────────────────────────────────────

	pub fn list_entries(&self) -> Result<Vec<String>, FfiError> {
		let inner = self.inner.lock().unwrap();
		Ok(
			inner
				.list(&self.branch)
				.map_err(FfiError::from)?
				.into_iter()
				.map(|n| n.to_string())
				.collect(),
		)
	}

	pub fn get_entry(&self, name: String) -> Result<Option<FfiItem>, FfiError> {
		let name = AccountName::new(&name).map_err(FfiError::from)?;
		let inner = self.inner.lock().unwrap();
		Ok(inner.get(&self.branch, &name).map_err(FfiError::from)?.map(FfiItem::from))
	}

	// ── write ─────────────────────────────────────────────────────────────────

	pub fn add_entry(&self, name: String, item: FfiItem, message: String) -> Result<(), FfiError> {
		let name = AccountName::new(&name).map_err(FfiError::from)?;
		let item = crate::models::Item::try_from(item)?;
		let msg = if message.is_empty() { format!("add {name}") } else { message };
		let inner = self.inner.lock().unwrap();
		inner.insert(&self.branch, name, item, &msg).map_err(FfiError::from)
	}

	pub fn update_entry(&self, name: String, item: FfiItem, message: String) -> Result<(), FfiError> {
		let name = AccountName::new(&name).map_err(FfiError::from)?;
		let item = crate::models::Item::try_from(item)?;
		let msg = if message.is_empty() { format!("update {name}") } else { message };
		let inner = self.inner.lock().unwrap();
		inner.update(&self.branch, &name, item, &msg).map_err(FfiError::from)
	}

	pub fn remove_entry(&self, name: String, message: String) -> Result<bool, FfiError> {
		let name = AccountName::new(&name).map_err(FfiError::from)?;
		let msg = if message.is_empty() { format!("remove {name}") } else { message };
		let inner = self.inner.lock().unwrap();
		inner.remove(&self.branch, &name, &msg).map_err(FfiError::from)
	}

	// ── history ───────────────────────────────────────────────────────────────

	pub fn log_history(&self, entry_filter: Option<String>) -> Result<Vec<FfiChangeEntry>, FfiError> {
		let filter = entry_filter.map(|n| AccountName::new(&n).map_err(FfiError::from)).transpose()?;
		let inner = self.inner.lock().unwrap();
		Ok(
			inner
				.log_impl(&self.branch, filter.as_ref())
				.map_err(FfiError::from)?
				.into_iter()
				.map(FfiChangeEntry::from)
				.collect(),
		)
	}

	pub fn revert_entry(&self, name: String, to_hash: String) -> Result<(), FfiError> {
		let name = AccountName::new(&name).map_err(FfiError::from)?;
		let hash = pijul_at_core::Hash::from_base32(to_hash.as_bytes())
			.ok_or_else(|| FfiError::Other { msg: format!("invalid hash: {to_hash}") })?;
		let inner = self.inner.lock().unwrap();
		inner.entry(&self.branch, name).revert_to(&hash).map_err(FfiError::from)
	}

	/// `to_hash`: pass `None` to diff against the current on-disk state.
	pub fn diff_entry(
		&self,
		name: String,
		from_hash: String,
		to_hash: Option<String>,
	) -> Result<DiffResult, FfiError> {
		let name = AccountName::new(&name).map_err(FfiError::from)?;
		let from = pijul_at_core::Hash::from_base32(from_hash.as_bytes())
			.ok_or_else(|| FfiError::Other { msg: format!("invalid hash: {from_hash}") })?;
		let to = to_hash
			.map(|h| {
				pijul_at_core::Hash::from_base32(h.as_bytes())
					.ok_or_else(|| FfiError::Other { msg: format!("invalid hash: {h}") })
			})
			.transpose()?;
		let inner = self.inner.lock().unwrap();
		inner.entry(&self.branch, name).diff(&from, to.as_ref()).map_err(FfiError::from)
	}

	pub fn head_hash(&self, name: String) -> Result<Option<String>, FfiError> {
		let name = AccountName::new(&name).map_err(FfiError::from)?;
		let inner = self.inner.lock().unwrap();
		Ok(
			inner
				.entry(&self.branch, name)
				.head()
				.map_err(FfiError::from)?
				.map(|h: pijul_at_core::Hash| h.to_base32()),
		)
	}
}
