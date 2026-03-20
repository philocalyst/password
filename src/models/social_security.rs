use celes::Country;
use human_name::Name;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use color_eyre::eyre::Result;

pub fn deserialize_name<'de, D>(deserializer: D) -> Result<Option<Name>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s: Option<String> = Option::deserialize(deserializer)?;
	match s {
		Some(name_str) => Ok(Name::parse(&name_str)),
		None => Ok(None),
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SocialSecurity {
	pub account_number:   String,
	#[serde(deserialize_with = "deserialize_name", default)]
	#[serde(skip_serializing)]
	pub legal_name:       Option<Name>,
	pub issuance_date:    Option<Date>,
	pub country_of_issue: Option<Country>,
}
