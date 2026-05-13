use crate::models::AccountName;

#[derive(thiserror::Error, Debug)]
pub enum Error {
	/// The requested entry does not exist in the store.
	#[error("entry not found: {name}")]
	EntryNotFound { name: AccountName },

	/// An entry with this name already exists (use `update` to overwrite).
	#[error("entry already exists: {name}")]
	EntryAlreadyExists { name: AccountName },

	/// The supplied account name is invalid (e.g. empty, contains slashes).
	#[error("invalid account name: {0}")]
	InvalidAccountName(String),

	/// Attempted to record a patch but the working copy had no changes.
	#[error("nothing to record")]
	NothingToRecord,

	/// A referenced patch hash is not in the channel's history.
	#[error("patch not found: {hash}")]
	PatchNotFound { hash: String },

	/// A revert could not be applied cleanly (conflict).
	#[error("revert conflict: {0}")]
	RevertConflict(String),

	/// An error originating inside libpijul.
	#[error("libpijul: {0}")]
	Pijul(String),

	/// The channel or repository could not be initialised.
	#[error("repo init: {0}")]
	RepoInit(String),

	/// An error propagated from the iroh stack.
	#[error("iroh: {0}")]
	Iroh(#[from] anyhow::Error),

	/// The provided ticket string could not be parsed.
	#[error("invalid ticket: {0}")]
	InvalidTicket(String),

	/// The sync operation did not complete within the allotted time.
	#[error("sync timed out after {secs}s")]
	SyncTimeout { secs: u64 },

	/// The remote peer closed the connection unexpectedly.
	#[error("peer disconnected")]
	PeerDisconnected,

	/// TOML serialisation error.
	#[error("serialize: {0}")]
	Serialize(#[from] toml::ser::Error),

	/// TOML deserialisation error.
	#[error("deserialize: {0}")]
	Deserialize(#[from] toml::de::Error),

	/// JSON serialisation / deserialisation error (used for schema payloads).
	#[error("json: {0}")]
	Json(#[from] serde_json::Error),

	/// Raw UTF-8 decoding failure.
	#[error("utf-8: {0}")]
	Utf8(#[from] std::str::Utf8Error),

	/// Generic I/O error.
	#[error("io: {0}")]
	Io(#[from] std::io::Error),

	/// A field failed a domain-level validation rule.
	#[error("validation — {field}: {reason}")]
	Validation { field: String, reason: String },

	/// A required field was absent.
	#[error("missing required field: {field}")]
	MissingField { field: String },
}

/// Convenience alias used throughout the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
