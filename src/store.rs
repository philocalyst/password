use pijul_at_core::Hash;

use crate::{models::{AccountName, Item, PasswordStore}, versioning::ChangeEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp {
	Retain,
	Insert,
	Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
	pub op:      DiffOp,
	pub content: String,
}

#[derive(Debug, Clone)]
pub struct DiffResult {
	pub label: String,
	pub lines: Vec<DiffLine>,
}

/// Persistence layer for the credential store.
/// Every method takes a `branch` argument (string) which maps to a Pijul
/// channel for multi-party isolation.
pub trait StoreBackend {
	type Error: std::error::Error + Send + Sync + 'static;

	/// Explicitly initialize a branch/party context.
	fn init(&self, branch: &str) -> std::result::Result<(), Self::Error>;

	/// Load the entire store for a given branch.
	fn load(&self, branch: &str) -> std::result::Result<PasswordStore, Self::Error>;

	/// Persist the entire store (overwrites current state) for a branch.
	fn save(&self, branch: &str, store: &PasswordStore) -> std::result::Result<(), Self::Error>;

	/// Return a sorted list of all entry names in a branch.
	fn list(&self, branch: &str) -> std::result::Result<Vec<AccountName>, Self::Error>;

	/// Look up a single entry by name inside a branch.
	fn get(&self, branch: &str, name: &AccountName)
	-> std::result::Result<Option<Item>, Self::Error>;

	/// Insert a new entry on a branch; errors if the name already exists.
	fn insert(
		&self,
		branch: &str,
		name: AccountName,
		item: Item,
	) -> std::result::Result<(), Self::Error>;

	/// Overwrite an existing entry on a branch; errors if it does not exist.
	fn update(
		&self,
		branch: &str,
		name: &AccountName,
		item: Item,
	) -> std::result::Result<(), Self::Error>;

	/// Remove an entry from a branch.  Returns `true` when something was actually
	/// deleted.
	fn remove(&self, branch: &str, name: &AccountName) -> std::result::Result<bool, Self::Error>;
}

/// Per-entry history management backed by Pijul patches.
/// Every operation takes a `branch` to scope history to a specific
/// party/channel.
pub trait Versioned {
	type Error: std::error::Error + Send + Sync + 'static;

	/// Snapshot the current state of a single entry and return its patch hash.
	fn record_entry(
		&self,
		branch: &str,
		name: &AccountName,
		msg: &str,
	) -> std::result::Result<Hash, Self::Error>;

	/// Revert a single entry in a branch to the state it had **after** `target`
	/// was applied.
	fn revert_entry(
		&self,
		branch: &str,
		name: &AccountName,
		target: &Hash,
	) -> std::result::Result<(), Self::Error>;

	/// Chronological change log, optionally filtered to a single entry in a
	/// branch.
	fn log(
		&self,
		branch: &str,
		entry_filter: Option<&AccountName>,
	) -> std::result::Result<Vec<ChangeEntry>, Self::Error>;

	/// Structured diff of a single entry between `from` and `to` inside a branch.
	fn diff_entry(
		&self,
		branch: &str,
		name: &AccountName,
		from: &Hash,
		to: Option<&Hash>,
	) -> std::result::Result<DiffResult, Self::Error>;

	/// Return the content of one entry as it existed immediately after `at` was
	/// applied.
	fn show_entry_at(
		&self,
		branch: &str,
		name: &AccountName,
		at: &Hash,
	) -> std::result::Result<Option<Item>, Self::Error>;

	/// The hash of the most recently applied patch for a given entry.
	fn head_entry(
		&self,
		branch: &str,
		name: &AccountName,
	) -> std::result::Result<Option<Hash>, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct StorePayload(pub Vec<u8>);

impl StorePayload {
	pub fn into_inner(self) -> Vec<u8> { self.0 }

	pub fn as_bytes(&self) -> &[u8] { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareTicket(pub String);

impl ShareTicket {
	pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for ShareTicket {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}

impl std::str::FromStr for ShareTicket {
	type Err = crate::Error;

	fn from_str(s: &str) -> crate::Result<Self> {
		if s.is_empty() {
			return Err(crate::Error::InvalidTicket("ticket must not be empty".into()));
		}
		Ok(Self(s.to_owned()))
	}
}
