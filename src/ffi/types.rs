use super::error::FfiError;
/// FFI-safe mirrors of the internal credential types.
///
/// All domain-typed fields (URL, email, phone, date, country) are exposed as
/// plain Strings.  Validation of inbound data happens inside `PwdStore` before
/// it reaches the internal `OnlineAccount`/`SocialSecurity` types.
use crate::{models::{Item, OnlineAccount, OnlineAccountSecurityQuestionsItem, OnlineAccountSignInWithItem, OnlineAccountStatus, SocialSecurity}, versioning::ChangeEntry};

// ── item types
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, uniffi::Enum)]
pub enum FfiItem {
	OnlineAccount { account: FfiOnlineAccount },
	SocialSecurity { ssn: FfiSocialSecurity },
}

/// String-field mirror of [`OnlineAccount`].
///
/// All typed fields (email, URL, phone, date, enum) are their canonical string
/// representations (RFC 5322 email, absolute URI, E.164 phone, ISO 8601 date,
/// "Active"/"Deactivated"/"Suspended" status, "Google"/"Apple"/… provider).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct FfiOnlineAccount {
	pub username:           Option<String>,
	pub password:           Option<String>,
	pub email:              Option<String>,
	pub phone:              Option<String>,
	pub sign_in_with:       Option<Vec<String>>,
	pub status:             Option<String>,
	pub host_website:       Option<String>,
	pub login_pages:        Option<Vec<String>>,
	pub security_questions: Option<Vec<FfiSecurityQuestion>>,
	pub two_factor_enabled: Option<bool>,
	pub associated_items:   Option<Vec<String>>,
	pub date_created:       Option<String>,
	pub notes:              Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSecurityQuestion {
	pub question: String,
	pub answer:   String,
}

/// String-field mirror of [`SocialSecurity`].
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct FfiSocialSecurity {
	pub account_number:   String,
	pub legal_name:       Option<String>,
	pub issuance_date:    Option<String>,
	pub country_of_issue: Option<String>,
	pub notes:            Option<String>,
}

/// String-field mirror of [`ChangeEntry`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiChangeEntry {
	pub hash:       String,
	pub message:    String,
	pub timestamp:  String,
	pub author:     String,
	pub entry_name: Option<String>,
}

// ── internal → FFI
// ────────────────────────────────────────────────────────────

impl From<Item> for FfiItem {
	fn from(item: Item) -> Self {
		match item {
			Item::OnlineAccount(a) => FfiItem::OnlineAccount { account: a.into() },
			Item::SocialSecurity(s) => FfiItem::SocialSecurity { ssn: s.into() },
		}
	}
}

impl From<OnlineAccount> for FfiOnlineAccount {
	fn from(a: OnlineAccount) -> Self {
		Self {
			username:           a.username,
			password:           a.password,
			email:              a.email.as_ref().map(|e| e.to_string()),
			phone:              a
				.phone
				.as_ref()
				.map(|p| phonenumber::format(p).mode(phonenumber::Mode::E164).to_string()),
			sign_in_with:       a.sign_in_with.map(|v| v.into_iter().map(|s| s.to_string()).collect()),
			status:             a.status.as_ref().map(|s| s.to_string()),
			host_website:       a.host_website.as_ref().map(|u| u.to_string()),
			login_pages:        a.login_pages.map(|v| v.into_iter().map(|u| u.to_string()).collect()),
			security_questions: a.security_questions.map(|v| {
				v.into_iter()
					.map(|q| FfiSecurityQuestion { question: q.question, answer: q.answer })
					.collect()
			}),
			two_factor_enabled: a.two_factor_enabled,
			associated_items:   a.associated_items,
			date_created:       a.date_created.as_ref().map(|d| d.to_string()),
			notes:              a.notes,
		}
	}
}

impl From<SocialSecurity> for FfiSocialSecurity {
	fn from(s: SocialSecurity) -> Self {
		Self {
			account_number:   s.account_number.to_string(),
			legal_name:       s.legal_name,
			issuance_date:    s.issuance_date.as_ref().map(|d| d.to_string()),
			country_of_issue: s.country_of_issue.as_ref().map(country_alpha2),
			notes:            s.notes,
		}
	}
}

impl From<ChangeEntry> for FfiChangeEntry {
	fn from(e: ChangeEntry) -> Self {
		Self {
			hash:       e.hash,
			message:    e.message,
			timestamp:  e.timestamp.to_string(),
			author:     e.author,
			entry_name: e.entry_name.map(|n| n.to_string()),
		}
	}
}

// ── FFI → internal
// ────────────────────────────────────────────────────────────

impl TryFrom<FfiItem> for Item {
	type Error = FfiError;

	fn try_from(item: FfiItem) -> Result<Self, FfiError> {
		match item {
			FfiItem::OnlineAccount { account } => Ok(Item::OnlineAccount(account.try_into()?)),
			FfiItem::SocialSecurity { ssn } => Ok(Item::SocialSecurity(ssn.try_into()?)),
		}
	}
}

impl TryFrom<FfiOnlineAccount> for OnlineAccount {
	type Error = FfiError;

	fn try_from(a: FfiOnlineAccount) -> Result<Self, FfiError> {
		let email = a
			.email
			.map(|s| {
				s.parse::<email_address::EmailAddress>()
					.map_err(|e| FfiError::Other { msg: format!("invalid email: {e}") })
			})
			.transpose()?;

		let phone = a
			.phone
			.map(|s| {
				phonenumber::parse(None, &s)
					.map_err(|e| FfiError::Other { msg: format!("invalid phone: {e}") })
			})
			.transpose()?;

		let host_website = a
			.host_website
			.map(|s| {
				s.parse::<url::Url>().map_err(|e| FfiError::Other { msg: format!("invalid URL: {e}") })
			})
			.transpose()?;

		let login_pages = a
			.login_pages
			.map(|v| {
				v.into_iter()
					.map(|s| {
						s.parse::<url::Url>()
							.map_err(|e| FfiError::Other { msg: format!("invalid login URL: {e}") })
					})
					.collect::<Result<Vec<_>, _>>()
			})
			.transpose()?;

		let sign_in_with = a
			.sign_in_with
			.map(|v| {
				v.into_iter()
					.map(|s| {
						s.parse::<OnlineAccountSignInWithItem>()
							.map_err(|_| FfiError::Other { msg: format!("unknown provider: {s}") })
					})
					.collect::<Result<Vec<_>, _>>()
			})
			.transpose()?;

		let status = a
			.status
			.map(|s| {
				s.parse::<OnlineAccountStatus>()
					.map_err(|_| FfiError::Other { msg: format!("unknown status: {s}") })
			})
			.transpose()?;

		let date_created = a
			.date_created
			.map(|s| {
				s.parse::<jiff::civil::Date>()
					.map_err(|e| FfiError::Other { msg: format!("invalid date: {e}") })
			})
			.transpose()?;

		let security_questions = a
			.security_questions
			.map(|v| {
				v.into_iter()
					.map(|q| {
						Ok::<_, FfiError>(OnlineAccountSecurityQuestionsItem {
							question: q.question,
							answer:   q.answer,
						})
					})
					.collect::<Result<Vec<_>, _>>()
			})
			.transpose()?;

		Ok(OnlineAccount {
			username: a.username,
			password: a.password,
			email,
			phone,
			sign_in_with,
			status,
			host_website,
			login_pages,
			security_questions,
			two_factor_enabled: a.two_factor_enabled,
			associated_items: a.associated_items,
			date_created,
			notes: a.notes,
		})
	}
}

impl TryFrom<FfiSocialSecurity> for SocialSecurity {
	type Error = FfiError;

	fn try_from(s: FfiSocialSecurity) -> Result<Self, FfiError> {
		let account_number = s
			.account_number
			.parse::<crate::models::SocialSecurityAccountNumber>()
			.map_err(|e| FfiError::Other { msg: format!("invalid account number: {e}") })?;

		let issuance_date = s
			.issuance_date
			.map(|d| {
				d.parse::<jiff::civil::Date>()
					.map_err(|e| FfiError::Other { msg: format!("invalid date: {e}") })
			})
			.transpose()?;

		let country_of_issue = s
			.country_of_issue
			.map(|c| {
				c.parse::<celes::Country>()
					.map_err(|_| FfiError::Other { msg: format!("unknown country code: {c}") })
			})
			.transpose()?;

		Ok(SocialSecurity {
			account_number,
			legal_name: s.legal_name,
			issuance_date,
			country_of_issue,
			notes: s.notes,
		})
	}
}

// ── helpers
// ───────────────────────────────────────────────────────────────────

fn country_alpha2(c: &celes::Country) -> String {
	// celes serialises to its alpha-2 code via serde; roundtrip to extract it.
	serde_json::to_value(c)
		.ok()
		.and_then(|v| v.as_str().map(String::from))
		.unwrap_or_else(|| format!("{c}"))
}
