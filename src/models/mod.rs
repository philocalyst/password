use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub mod account;
pub mod social_security;

pub use account::*;
pub use social_security::*;

#[derive(Default, Serialize, Deserialize)]
pub struct PasswordStore {
	pub items: HashMap<String, Item>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Item {
	OnlineAccount(OnlineAccount),
	SocialSecurity(SocialSecurity),
}
