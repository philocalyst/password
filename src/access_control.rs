use std::{collections::HashMap, marker::PhantomData};

use keyhive_core::access::Access as KeyhiveAccess;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Error, Result, models::AccountName};

pub trait BranchKind: Clone + std::fmt::Debug + Send + Sync + 'static {
	const PREFIX: &'static str;
	const INHERITS_FROM_PARENTS: bool;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PersonalBranch;

impl BranchKind for PersonalBranch {
	const INHERITS_FROM_PARENTS: bool = false;
	const PREFIX: &'static str = "personal";
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupBranch;

impl BranchKind for GroupBranch {
	const INHERITS_FROM_PARENTS: bool = true;
	const PREFIX: &'static str = "group";
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct BranchSegment(String);

impl BranchSegment {
	pub fn new(raw: impl Into<String>) -> Result<Self> {
		let raw = raw.into();
		validate_branch_segment(&raw)?;
		Ok(Self(raw))
	}

	pub fn as_str(&self) -> &str { &self.0 }
}

impl TryFrom<&str> for BranchSegment {
	type Error = Error;

	fn try_from(value: &str) -> Result<Self> { Self::new(value) }
}

impl TryFrom<String> for BranchSegment {
	type Error = Error;

	fn try_from(value: String) -> Result<Self> { Self::new(value) }
}

impl std::fmt::Display for BranchSegment {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}

impl<'de> Deserialize<'de> for BranchSegment {
	fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let raw = String::deserialize(deserializer)?;
		Self::new(raw).map_err(de::Error::custom)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchPath<K: BranchKind> {
	canonical: String,
	segments:  Vec<BranchSegment>,
	_kind:     PhantomData<K>,
}

impl BranchPath<PersonalBranch> {
	pub fn personal(name: BranchSegment) -> Self { Self::from_segments(vec![name]) }
}

impl BranchPath<GroupBranch> {
	pub fn group<I>(segments: I) -> Result<Self>
	where
		I: IntoIterator<Item = BranchSegment>,
	{
		let segments: Vec<BranchSegment> = segments.into_iter().collect();
		if segments.is_empty() {
			return Err(Error::InvalidBranchName("group branch must not be empty".into()));
		}
		Ok(Self::from_segments(segments))
	}

	pub fn ancestors(&self) -> Vec<Self> {
		(1..=self.segments.len())
			.map(|end| Self::from_segments(self.segments[..end].to_vec()))
			.collect()
	}
}

impl<K: BranchKind> BranchPath<K> {
	fn from_segments(segments: Vec<BranchSegment>) -> Self {
		let path = segments.iter().map(BranchSegment::as_str).collect::<Vec<_>>().join("/");
		Self { canonical: format!("{}:{}", K::PREFIX, path), segments, _kind: PhantomData }
	}

	pub fn as_str(&self) -> &str { &self.canonical }

	pub fn segments(&self) -> &[BranchSegment] { &self.segments }

	pub fn storage_component(&self) -> String { branch_storage_component(self) }
}

impl<K: BranchKind> std::fmt::Display for BranchPath<K> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.canonical)
	}
}

impl<K: BranchKind> Serialize for BranchPath<K> {
	fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(&self.canonical)
	}
}

impl<'de, K: BranchKind> Deserialize<'de> for BranchPath<K> {
	fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let raw = String::deserialize(deserializer)?;
		let prefix = format!("{}:", K::PREFIX);
		let path = raw
			.strip_prefix(&prefix)
			.ok_or_else(|| de::Error::custom(format!("branch must start with {prefix}")))?;
		let segments: Vec<&str> = path.split('/').collect();
		if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
			return Err(de::Error::custom("branch path must not be empty"));
		}
		if !K::INHERITS_FROM_PARENTS && segments.len() != 1 {
			return Err(de::Error::custom("personal branches must contain exactly one segment"));
		}
		let segments = segments
			.into_iter()
			.map(BranchSegment::new)
			.collect::<Result<Vec<_>>>()
			.map_err(de::Error::custom)?;
		Ok(Self::from_segments(segments))
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrincipalId(String);

impl PrincipalId {
	pub fn new(raw: impl Into<String>) -> Result<Self> {
		let raw = raw.into();
		if raw.is_empty() {
			return Err(Error::AccessDenied("principal id must not be empty".into()));
		}
		Ok(Self(raw))
	}

	pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchTarget<K: BranchKind> {
	pub branch: BranchPath<K>,
}

impl<K: BranchKind> BranchTarget<K> {
	pub fn new(branch: BranchPath<K>) -> Self { Self { branch } }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemTarget<K: BranchKind> {
	pub branch: BranchPath<K>,
	pub name:   AccountName,
}

impl<K: BranchKind> ItemTarget<K> {
	pub fn new(branch: BranchPath<K>, name: AccountName) -> Self { Self { branch, name } }
}

pub trait AccessLevel: Clone + std::fmt::Debug + Send + Sync + 'static {
	const REQUIRED: KeyhiveAccess;
}

#[derive(Debug, Clone)]
pub struct RelayAccess;
#[derive(Debug, Clone)]
pub struct ReadAccess;
#[derive(Debug, Clone)]
pub struct EditAccess;
#[derive(Debug, Clone)]
pub struct AdminAccess;

impl AccessLevel for RelayAccess {
	const REQUIRED: KeyhiveAccess = KeyhiveAccess::Relay;
}
impl AccessLevel for ReadAccess {
	const REQUIRED: KeyhiveAccess = KeyhiveAccess::Read;
}
impl AccessLevel for EditAccess {
	const REQUIRED: KeyhiveAccess = KeyhiveAccess::Edit;
}
impl AccessLevel for AdminAccess {
	const REQUIRED: KeyhiveAccess = KeyhiveAccess::Admin;
}

pub trait GrantsRead: AccessLevel {}
pub trait GrantsEdit: GrantsRead {}
pub trait GrantsAdmin: GrantsEdit {}

impl GrantsRead for ReadAccess {}
impl GrantsRead for EditAccess {}
impl GrantsRead for AdminAccess {}
impl GrantsEdit for EditAccess {}
impl GrantsEdit for AdminAccess {}
impl GrantsAdmin for AdminAccess {}

#[derive(Debug, Clone)]
pub struct Authorized<'a, T, L: AccessLevel> {
	principal: PrincipalId,
	target:    T,
	granted:   KeyhiveAccess,
	_scope:    PhantomData<&'a ()>,
	_level:    PhantomData<L>,
}

impl<T, L: AccessLevel> Authorized<'_, T, L> {
	pub fn principal(&self) -> &PrincipalId { &self.principal }

	pub fn target(&self) -> &T { &self.target }

	pub fn granted(&self) -> KeyhiveAccess { self.granted }
}

impl<K: BranchKind, L: AccessLevel> Authorized<'_, BranchTarget<K>, L> {
	pub fn branch(&self) -> &BranchPath<K> { &self.target.branch }
}

impl<K: BranchKind, L: AccessLevel> Authorized<'_, ItemTarget<K>, L> {
	pub fn branch(&self) -> &BranchPath<K> { &self.target.branch }

	pub fn name(&self) -> &AccountName { &self.target.name }
}

pub trait AccessControl {
	fn authorize_branch<'a, K: BranchKind, L: AccessLevel>(
		&'a self,
		principal: &PrincipalId,
		branch: &BranchPath<K>,
	) -> Result<Authorized<'a, BranchTarget<K>, L>>;

	fn authorize_item<'a, K: BranchKind, L: AccessLevel>(
		&'a self,
		principal: &PrincipalId,
		item: &ItemTarget<K>,
	) -> Result<Authorized<'a, ItemTarget<K>, L>>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryAccessControl {
	branch_grants: HashMap<(String, String), KeyhiveAccess>,
	item_grants:   HashMap<(String, String, AccountName), KeyhiveAccess>,
	policy_epoch:  u64,
}

impl InMemoryAccessControl {
	pub fn policy_epoch(&self) -> u64 { self.policy_epoch }

	pub fn grant_branch<K: BranchKind>(
		&mut self,
		principal: &PrincipalId,
		branch: &BranchPath<K>,
		access: KeyhiveAccess,
	) {
		self.branch_grants.insert((principal.as_str().to_owned(), branch.as_str().to_owned()), access);
		self.policy_epoch = self.policy_epoch.saturating_add(1);
	}

	pub fn revoke_branch<K: BranchKind>(&mut self, principal: &PrincipalId, branch: &BranchPath<K>) {
		self.branch_grants.remove(&(principal.as_str().to_owned(), branch.as_str().to_owned()));
		self.policy_epoch = self.policy_epoch.saturating_add(1);
	}

	pub fn grant_item<K: BranchKind>(
		&mut self,
		principal: &PrincipalId,
		item: &ItemTarget<K>,
		access: KeyhiveAccess,
	) {
		self.item_grants.insert(
			(principal.as_str().to_owned(), item.branch.as_str().to_owned(), item.name.clone()),
			access,
		);
		self.policy_epoch = self.policy_epoch.saturating_add(1);
	}

	pub fn revoke_item<K: BranchKind>(&mut self, principal: &PrincipalId, item: &ItemTarget<K>) {
		self.item_grants.remove(&(
			principal.as_str().to_owned(),
			item.branch.as_str().to_owned(),
			item.name.clone(),
		));
		self.policy_epoch = self.policy_epoch.saturating_add(1);
	}

	fn branch_grant<K: BranchKind>(
		&self,
		principal: &PrincipalId,
		branch: &BranchPath<K>,
	) -> Option<KeyhiveAccess> {
		if K::INHERITS_FROM_PARENTS {
			let group = BranchPath::<GroupBranch>::from_segments(branch.segments().to_vec());
			group.ancestors().into_iter().rev().find_map(|ancestor| {
				self
					.branch_grants
					.get(&(principal.as_str().to_owned(), ancestor.as_str().to_owned()))
					.copied()
			})
		} else {
			self.branch_grants.get(&(principal.as_str().to_owned(), branch.as_str().to_owned())).copied()
		}
	}
}

impl AccessControl for InMemoryAccessControl {
	fn authorize_branch<'a, K: BranchKind, L: AccessLevel>(
		&'a self,
		principal: &PrincipalId,
		branch: &BranchPath<K>,
	) -> Result<Authorized<'a, BranchTarget<K>, L>> {
		let granted =
			self.branch_grant(principal, branch).filter(|granted| *granted >= L::REQUIRED).ok_or_else(
				|| Error::AccessDenied(format!("{principal:?} lacks {} for {branch}", L::REQUIRED)),
			)?;

		Ok(Authorized {
			principal: principal.clone(),
			target: BranchTarget::new(branch.clone()),
			granted,
			_scope: PhantomData,
			_level: PhantomData,
		})
	}

	fn authorize_item<'a, K: BranchKind, L: AccessLevel>(
		&'a self,
		principal: &PrincipalId,
		item: &ItemTarget<K>,
	) -> Result<Authorized<'a, ItemTarget<K>, L>> {
		let direct = self
			.item_grants
			.get(&(principal.as_str().to_owned(), item.branch.as_str().to_owned(), item.name.clone()))
			.copied();
		let inherited = self.branch_grant(principal, &item.branch);
		let granted = direct
			.into_iter()
			.chain(inherited)
			.max()
			.filter(|granted| *granted >= L::REQUIRED)
			.ok_or_else(|| {
				Error::AccessDenied(format!(
					"{principal:?} lacks {} for {}/{}",
					L::REQUIRED,
					item.branch,
					item.name
				))
			})?;

		Ok(Authorized {
			principal: principal.clone(),
			target: item.clone(),
			granted,
			_scope: PhantomData,
			_level: PhantomData,
		})
	}
}

pub fn branch_storage_component<K: BranchKind>(branch: &BranchPath<K>) -> String {
	branch_storage_component_raw(branch.as_str())
}

pub(crate) fn branch_storage_component_raw(branch: &str) -> String {
	let mut escaped = String::new();
	for byte in branch.bytes() {
		match byte {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => escaped.push(byte as char),
			_ => escaped.push_str(&format!("%{byte:02X}")),
		}
	}
	escaped
}

fn validate_branch_segment(segment: &str) -> Result<()> {
	if segment.is_empty() {
		return Err(Error::InvalidBranchName("branch segments must not be empty".into()));
	}
	if segment == "." || segment == ".." {
		return Err(Error::InvalidBranchName("branch segments must not be dot paths".into()));
	}
	if segment.contains('/') || segment.contains('\\') {
		return Err(Error::InvalidBranchName(
			"branch segments must not contain path separators".into(),
		));
	}
	if segment.contains("..") {
		return Err(Error::InvalidBranchName("branch segments must not contain '..'".into()));
	}
	let lower = segment.to_ascii_lowercase();
	for encoded_separator in ["%2f", "%5c", "%00"] {
		if lower.contains(encoded_separator) {
			return Err(Error::InvalidBranchName(
				"branch segments must not contain encoded separators or NUL bytes".into(),
			));
		}
	}
	Ok(())
}
