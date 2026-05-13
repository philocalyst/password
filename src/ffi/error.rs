use crate::Error as PwdError;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
	#[error("entry not found: {name}")]
	EntryNotFound { name: String },

	#[error("entry already exists: {name}")]
	EntryAlreadyExists { name: String },

	#[error("invalid account name: {msg}")]
	InvalidAccountName { msg: String },

	#[error("invalid ticket: {msg}")]
	InvalidTicket { msg: String },

	#[error("nothing to record")]
	NothingToRecord,

	#[error("patch not found: {hash}")]
	PatchNotFound { hash: String },

	#[error("io: {msg}")]
	Io { msg: String },

	#[error("{msg}")]
	Other { msg: String },
}

impl From<PwdError> for FfiError {
	fn from(e: PwdError) -> Self {
		match e {
			PwdError::EntryNotFound { name } => Self::EntryNotFound { name: name.to_string() },
			PwdError::EntryAlreadyExists { name } => Self::EntryAlreadyExists { name: name.to_string() },
			PwdError::InvalidAccountName(msg) => Self::InvalidAccountName { msg },
			PwdError::InvalidTicket(msg) => Self::InvalidTicket { msg },
			PwdError::NothingToRecord => Self::NothingToRecord,
			PwdError::PatchNotFound { hash } => Self::PatchNotFound { hash },
			PwdError::Io(e) => Self::Io { msg: e.to_string() },
			other => Self::Other { msg: other.to_string() },
		}
	}
}
