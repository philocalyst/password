use pijul_at_core::Hash;

use crate::{access_control::{BranchKind, BranchPath}, models::{AccountName, Item, PasswordStore}, versioning::ChangeEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreChange {
	AddEntry { name: AccountName, kind: &'static str },
	UpdateEntry { name: AccountName, fields: Vec<String> },
	RemoveEntry { name: AccountName },
	RekeyStore { entries: Vec<AccountName> },
	ReceiveEntries { count: usize },
	RevertEntry { name: AccountName },
	Custom(String),
}

impl StoreChange {
	pub fn add_entry(name: AccountName, item: &Item) -> Self {
		Self::AddEntry { name, kind: item.kind_str() }
	}

	pub fn update_entry(
		name: AccountName,
		fields: impl IntoIterator<Item = impl Into<String>>,
	) -> Self {
		Self::UpdateEntry { name, fields: fields.into_iter().map(Into::into).collect() }
	}

	pub fn remove_entry(name: AccountName) -> Self { Self::RemoveEntry { name } }

	pub fn rekey_store(entries: impl IntoIterator<Item = AccountName>) -> Self {
		Self::RekeyStore { entries: entries.into_iter().collect() }
	}

	pub fn message(&self) -> String {
		match self {
			StoreChange::AddEntry { name, kind } => format!("add {kind} entry: {name}"),
			StoreChange::UpdateEntry { name, fields } if fields.is_empty() => {
				format!("update entry: {name}")
			}
			StoreChange::UpdateEntry { name, fields } => {
				format!("update entry: {name} ({})", fields.join(", "))
			}
			StoreChange::RemoveEntry { name } => format!("remove entry: {name}"),
			StoreChange::RekeyStore { entries } if entries.is_empty() => "rekey store".to_owned(),
			StoreChange::RekeyStore { entries } => format!(
				"rekey store: {}",
				entries.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
			),
			StoreChange::ReceiveEntries { count } => format!("receive {count} entries"),
			StoreChange::RevertEntry { name } => format!("revert entry: {name}"),
			StoreChange::Custom(message) => message.clone(),
		}
	}

	pub fn entry_name(&self) -> Option<&AccountName> {
		match self {
			StoreChange::AddEntry { name, .. }
			| StoreChange::UpdateEntry { name, .. }
			| StoreChange::RemoveEntry { name }
			| StoreChange::RevertEntry { name } => Some(name),
			StoreChange::RekeyStore { .. }
			| StoreChange::ReceiveEntries { .. }
			| StoreChange::Custom(_) => None,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DiffOp {
	Retain,
	Insert,
	Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DiffLine {
	pub op:      DiffOp,
	pub content: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DiffResult {
	pub label: String,
	pub lines: Vec<DiffLine>,
}

/// Persistence layer for the credential store.
pub trait StoreBackend {
	type Error: std::error::Error + Send + Sync + 'static;

	fn init<K: BranchKind>(&self, branch: &BranchPath<K>) -> std::result::Result<(), Self::Error>;

	fn load<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
	) -> std::result::Result<PasswordStore, Self::Error>;
	fn save<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		store: &PasswordStore,
	) -> std::result::Result<(), Self::Error>;

	fn list<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
	) -> std::result::Result<Vec<AccountName>, Self::Error>;
	fn get<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: &AccountName,
	) -> std::result::Result<Option<Item>, Self::Error>;

	/// Insert and immediately snapshot the entry as a Pijul patch.
	fn insert<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: AccountName,
		item: Item,
		change: StoreChange,
	) -> std::result::Result<(), Self::Error>;

	/// Update and immediately snapshot the entry as a Pijul patch.
	fn update<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: &AccountName,
		item: Item,
		change: StoreChange,
	) -> std::result::Result<(), Self::Error>;

	/// Remove and snapshot the deletion as a Pijul patch.
	fn remove<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: &AccountName,
		change: StoreChange,
	) -> std::result::Result<bool, Self::Error>;
}

/// History operations scoped to a **single entry** in a branch.
///
/// Obtain a handle via [`PijulStore::entry`]; the handle borrows the store,
/// so there are no branch/name parameters on individual methods.
pub trait VersionedEntry {
	type Error: std::error::Error + Send + Sync + 'static;

	/// Revert this entry to the state it had after `target` was applied.
	fn revert_to(&self, target: &Hash) -> std::result::Result<(), Self::Error>;

	/// Chronological change log for this entry.
	fn log(&self) -> std::result::Result<Vec<ChangeEntry>, Self::Error>;

	/// Structured diff between `from` and `to` (or current state if `to` is
	/// `None`).
	fn diff(&self, from: &Hash, to: Option<&Hash>) -> std::result::Result<DiffResult, Self::Error>;

	/// The entry's content as it was immediately after patch `at` was applied.
	fn snapshot_at(&self, at: &Hash) -> std::result::Result<Option<Item>, Self::Error>;

	/// Hash of the most recent patch for this entry.
	fn head(&self) -> std::result::Result<Option<Hash>, Self::Error>;
}

// ── P2P payload types
// ─────────────────────────────────────────────────────────

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
