pub mod error;
pub mod ffi;
pub mod models;
pub mod p2p;
pub mod store;
pub mod versioning;

pub use error::{Error, Result};
pub use models::{AccountName, Item, PasswordStore};
pub use store::{ShareTicket, StoreBackend, StorePayload, VersionedEntry};
pub use versioning::{ChangeEntry, EntryHandle, PijulStore};

uniffi::setup_scaffolding!();
