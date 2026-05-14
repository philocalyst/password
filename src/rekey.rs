use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// A normalized master identity, following agenix-rekey's
/// `{ identity, pubkey? }` shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterIdentity {
	pub identity: PathBuf,
	pub pubkey:   Option<String>,
}

impl MasterIdentity {
	pub fn new(identity: impl Into<PathBuf>) -> Self {
		Self { identity: identity.into(), pubkey: None }
	}

	pub fn with_pubkey(identity: impl Into<PathBuf>, pubkey: impl Into<String>) -> Self {
		Self { identity: identity.into(), pubkey: Some(pubkey.into()) }
	}
}

/// Rekey configuration shared by concrete encryption backends.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MasterKeySet {
	pub master_identities:        Vec<MasterIdentity>,
	pub extra_encryption_pubkeys: Vec<String>,
	pub primary_identity_pubkey:  Option<String>,
	pub primary_identity_only:    bool,
}

impl MasterKeySet {
	pub fn decryption_order(&self) -> Vec<&MasterIdentity> {
		let mut ordered = Vec::new();
		if let Some(primary) = &self.primary_identity_pubkey {
			if let Some(identity) =
				self.master_identities.iter().find(|identity| identity.pubkey.as_ref() == Some(primary))
			{
				ordered.push(identity);
				if self.primary_identity_only {
					return ordered;
				}
			}
		}

		ordered.extend(self.master_identities.iter().filter(|identity| {
			Some(identity.pubkey.as_ref()) != self.primary_identity_pubkey.as_ref().map(Some)
		}));

		ordered
	}

	pub fn recipients(&self) -> Vec<String> {
		let mut recipients: Vec<String> =
			self.master_identities.iter().filter_map(|identity| identity.pubkey.clone()).collect();

		recipients.extend(self.extra_encryption_pubkeys.iter().cloned());
		recipients.sort();
		recipients.dedup();
		recipients
	}
}

/// Offline session refresh policy. `None` means operation-count expiry is
/// ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflineSessionPolicy {
	pub max_operations: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflineSession {
	pub subject:             String,
	pub policy_epoch:        u64,
	pub issued_at_operation: u64,
}

impl OfflineSessionPolicy {
	pub const DEFAULT_MAX_OPERATIONS: u64 = 10_000;

	pub fn ignored() -> Self { Self { max_operations: None } }

	pub fn issue(
		&self,
		subject: impl Into<String>,
		policy_epoch: u64,
		current_operation: u64,
	) -> OfflineSession {
		OfflineSession { subject: subject.into(), policy_epoch, issued_at_operation: current_operation }
	}

	pub fn require_fresh(
		&self,
		session: &OfflineSession,
		current_policy_epoch: u64,
		current_operation: u64,
	) -> Result<()> {
		if session.policy_epoch != current_policy_epoch {
			return Err(Error::Session("access policy changed; re-initialize to continue".into()));
		}

		let Some(max_operations) = self.max_operations else {
			return Ok(());
		};
		if current_operation.saturating_sub(session.issued_at_operation) > max_operations {
			return Err(Error::Session(format!(
				"session exceeded {max_operations} operations; re-initialize to continue"
			)));
		}
		Ok(())
	}
}

impl Default for OfflineSessionPolicy {
	fn default() -> Self { Self { max_operations: Some(Self::DEFAULT_MAX_OPERATIONS) } }
}
