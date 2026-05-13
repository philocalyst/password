// Pull in the typify-generated structs/enums.
include!(concat!(env!("OUT_DIR"), "/models_generated.rs"));

use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};

pub type AccountStatus = OnlineAccountStatus;
pub type AuthProvider = OnlineAccountSignInWithItem;

/// A validated, non-empty identifier for a store entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountName(String);

impl AccountName {
	/// Parse and validate an account name.
	pub fn new(raw: impl Into<String>) -> crate::Result<Self> {
		let s = raw.into();
		if s.is_empty() {
			return Err(crate::Error::InvalidAccountName("name must not be empty".into()));
		}
		if s.len() > 255 {
			return Err(crate::Error::InvalidAccountName("name exceeds 255 bytes".into()));
		}
		for ch in ['/', '\\', '.'] {
			if s.contains(ch) {
				return Err(crate::Error::InvalidAccountName(format!("name must not contain '{ch}'")));
			}
		}
		Ok(Self(s))
	}

	/// Return the inner string slice.
	pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for AccountName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl TryFrom<String> for AccountName {
	type Error = crate::Error;

	fn try_from(s: String) -> crate::Result<Self> { Self::new(s) }
}

impl TryFrom<&str> for AccountName {
	type Error = crate::Error;

	fn try_from(s: &str) -> crate::Result<Self> { Self::new(s) }
}

impl AsRef<str> for AccountName {
	fn as_ref(&self) -> &str { &self.0 }
}

/// The sum type over all storable credential kinds.
///
/// New kinds can be added by adding a new JSON Schema file under `schemas/`
/// and mapping it here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
	OnlineAccount(OnlineAccount),
	SocialSecurity(SocialSecurity),
}

impl Item {
	/// Return the item kind as a human-readable string.
	pub fn kind_str(&self) -> &'static str {
		match self {
			Item::OnlineAccount(_) => "online_account",
			Item::SocialSecurity(_) => "social_security",
		}
	}
}

/// The root in-memory store; a map from validated names to credential items.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PasswordStore {
	pub items: HashMap<AccountName, Item>,
}

impl PasswordStore {
	pub fn new() -> Self { Self::default() }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn account_name_rejects_empty() {
		assert!(AccountName::new("").is_err());
	}

	#[test]
	fn account_name_rejects_path_chars() {
		assert!(AccountName::new("foo/bar").is_err());
		assert!(AccountName::new("foo.bar").is_err());
		assert!(AccountName::new("foo\\bar").is_err());
	}

	#[test]
	fn account_name_accepts_valid() {
		let n = AccountName::new("github-personal").unwrap();
		assert_eq!(n.as_str(), "github-personal");
	}

	#[test]
	fn account_name_rejects_too_long() {
		let long = "a".repeat(256);
		assert!(AccountName::new(long).is_err());
	}

	#[test]
	fn item_kind_str() { let _store = PasswordStore::new(); }
}
