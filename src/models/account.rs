use email_address::EmailAddress;
use jiff::civil::Date;
use phonenumber::PhoneNumber;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Serialize, Deserialize, Clone)]
pub struct OnlineAccount {
	#[serde(skip)]
	pub account:            String,
	pub username:           Option<String>,
	pub email:              Option<EmailAddress>,
	pub phone:              Option<PhoneNumber>,
	pub sign_in_with:       Option<Vec<AuthProvider>>,
	pub password:           Option<String>,
	pub status:             Option<AccountStatus>,
	pub host_website:       Option<Url>,
	pub login_pages:        Option<Vec<Url>>,
	pub security_questions: Option<Vec<SecurityQuestion>>,
	pub date_created:       Option<Date>,
	pub two_factor_enabled: Option<bool>,
	pub associated_items:   Option<Vec<String>>,
	pub notes:              Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum AuthProvider {
	Google,
	Apple,
	Facebook,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum AccountStatus {
	Active,
	Deactivated,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SecurityQuestion {
	pub question: String,
	pub answer:   String,
}
