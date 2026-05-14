//! Integration tests for `StoreBackend` + `Versioned` via `PijulStore`.
//! Testing complex versioning, branching, and reverting scenarios.

use password::{AccessControl, AccountName, AgeScrypt, BranchPath, BranchSegment, EditAccess, EncryptionMethod, Error, GroupBranch, InMemoryAccessControl, Item, ItemTarget, PersonalBranch, PijulStore, PrincipalId, ReadAccess, StoreBackend, VersionedEntry, branch_storage_component, models::{AccountStatus, OnlineAccount}};

fn sample_account(pass: &str) -> Item {
	Item::OnlineAccount(OnlineAccount {
		username:           Some("alice".into()),
		password:           Some(pass.into()),
		email:              None,
		phone:              None,
		sign_in_with:       None,
		status:             Some(AccountStatus::Active),
		host_website:       None,
		login_pages:        None,
		security_questions: None,
		date_created:       None,
		two_factor_enabled: Some(false),
		associated_items:   None,
		notes:              None,
	})
}

fn name(s: &str) -> AccountName { AccountName::new(s).unwrap() }

fn store() -> password::versioning::PijulStore<password::Unlocked<AgeScrypt>> {
	PijulStore::ephemeral().unwrap().unlock_with(AgeScrypt::new("test-passphrase").unwrap())
}

fn branch_segment(raw: &str) -> BranchSegment { BranchSegment::new(raw).unwrap() }

fn personal_branch(raw: &str) -> BranchPath<PersonalBranch> {
	BranchPath::personal(branch_segment(raw))
}

fn main_branch() -> BranchPath<PersonalBranch> { personal_branch("main") }

fn group_branch<const N: usize>(segments: [&str; N]) -> BranchPath<GroupBranch> {
	BranchPath::<GroupBranch>::group(segments.into_iter().map(branch_segment)).unwrap()
}

fn add_change(name: &AccountName) -> password::StoreChange {
	password::StoreChange::AddEntry { name: name.clone(), kind: "online_account" }
}

fn update_change(name: &AccountName, fields: &[&str]) -> password::StoreChange {
	password::StoreChange::update_entry(name.clone(), fields.iter().copied())
}

fn rekey_change(entries: &[AccountName]) -> password::StoreChange {
	password::StoreChange::rekey_store(entries.iter().cloned())
}

#[test]
fn branches_are_isolated() {
	let store = store();
	let n1 = name("github");
	let alice = personal_branch("alice");
	let bob = personal_branch("bob");

	// Insert into 'alice' branch
	store.insert(&alice, n1.clone(), sample_account("alice_pass"), add_change(&n1)).unwrap();

	// Insert into 'bob' branch
	store.insert(&bob, n1.clone(), sample_account("bob_pass"), add_change(&n1)).unwrap();

	// Verify isolation
	if let Item::OnlineAccount(a) = store.get(&alice, &n1).unwrap().unwrap() {
		assert_eq!(a.password.as_deref(), Some("alice_pass"));
	}
	if let Item::OnlineAccount(b) = store.get(&bob, &n1).unwrap().unwrap() {
		assert_eq!(b.password.as_deref(), Some("bob_pass"));
	}
}

#[test]
fn reverting_entry_preserves_other_entries_across_interleaved_history() {
	let store = store();
	let n1 = name("github");
	let n2 = name("bank");

	// v1: github created
	store.insert(&main_branch(), n1.clone(), sample_account("gh_pass_1"), add_change(&n1)).unwrap();
	let hash_gh_1 = store.entry(&main_branch(), n1.clone()).head().unwrap().unwrap();

	// v2: bank created
	store.insert(&main_branch(), n2.clone(), sample_account("bank_pass"), add_change(&n2)).unwrap();

	// v3: github updated
	store
		.update(&main_branch(), &n1, sample_account("gh_pass_2"), update_change(&n1, &["password"]))
		.unwrap();

	// Now we want to revert 'github' back to hash_gh_1.
	// This MUST NOT affect 'bank' which was added in between (hash_bank).
	store.entry(&main_branch(), n1.clone()).revert_to(&hash_gh_1).unwrap();

	// Verify github is reverted
	if let Item::OnlineAccount(a) = store.get(&main_branch(), &n1).unwrap().unwrap() {
		assert_eq!(a.password.as_deref(), Some("gh_pass_1"));
	}

	// Verify bank is untouched
	if let Item::OnlineAccount(a) = store.get(&main_branch(), &n2).unwrap().unwrap() {
		assert_eq!(a.password.as_deref(), Some("bank_pass"));
	}
}

#[test]
fn entries_are_encrypted_at_rest_and_require_unlock() {
	let store = store();
	let n1 = name("github");
	store.insert(&main_branch(), n1.clone(), sample_account("gh_pass_1"), add_change(&n1)).unwrap();

	let path = store.store_dir.join("branches/personal%3Amain/github.toml.age");
	let bytes = std::fs::read(path).unwrap();
	let text = String::from_utf8_lossy(&bytes);
	assert!(!text.contains("gh_pass_1"));
	assert!(!text.contains("online_account"));
}

#[test]
fn rekey_rotates_the_store_passphrase() {
	let store = store();
	let n1 = name("github");
	store.insert(&main_branch(), n1.clone(), sample_account("gh_pass_1"), add_change(&n1)).unwrap();
	let path = store.store_dir.clone();

	let store = store
		.rekey_with(
			&main_branch(),
			AgeScrypt::new("new-passphrase").unwrap(),
			rekey_change(&[n1.clone()]),
		)
		.unwrap();
	assert!(store.get(&main_branch(), &n1).unwrap().is_some());

	let locked = PijulStore::open(path).unwrap();
	let old = locked.unlock_with(AgeScrypt::new("test-passphrase").unwrap());
	assert!(old.get(&main_branch(), &n1).is_err());
}

#[test]
fn offline_session_policy_ignores_wall_clock_and_tracks_policy_epoch() {
	let policy = password::OfflineSessionPolicy { max_operations: Some(2) };
	let session = policy.issue("main", 7, 100);

	assert!(policy.require_fresh(&session, 7, 102).is_ok());
	assert!(policy.require_fresh(&session, 7, 103).is_err());
	assert!(policy.require_fresh(&session, 8, 101).is_err());

	let ignored = password::OfflineSessionPolicy::ignored();
	assert!(ignored.require_fresh(&session, 7, u64::MAX).is_ok());
	assert!(ignored.require_fresh(&session, 6, 101).is_err());
}

#[test]
fn structured_store_changes_include_entry_and_field_context() {
	let github = name("github");
	assert_eq!(
		password::StoreChange::AddEntry { name: github.clone(), kind: "online_account" }.message(),
		"add online_account entry: github"
	);
	assert_eq!(
		password::StoreChange::update_entry(github.clone(), ["password", "two_factor_enabled"])
			.message(),
		"update entry: github (password, two_factor_enabled)"
	);
	assert_eq!(password::StoreChange::rekey_store([github]).message(), "rekey store: github");
}

#[test]
fn structured_store_changes_must_match_the_mutated_entry() {
	let store = store();
	let github = name("github");
	let bank = name("bank");
	let mismatched_change = password::StoreChange::add_entry(github, &sample_account("bank_pass"));

	let err =
		store.insert(&main_branch(), bank, sample_account("bank_pass"), mismatched_change).unwrap_err();
	assert!(err.to_string().contains("change targets github"));
}

#[test]
fn adversarial_wrong_passphrase_errors_instead_of_returning_empty_store() {
	let store = store();
	let n1 = name("github");
	store.insert(&main_branch(), n1.clone(), sample_account("gh_pass_1"), add_change(&n1)).unwrap();
	let path = store.store_dir.clone();

	let wrong_key_store =
		PijulStore::open(path).unwrap().unlock_with(AgeScrypt::new("wrong-passphrase").unwrap());

	assert!(wrong_key_store.list(&main_branch()).is_err());
	assert!(wrong_key_store.get(&main_branch(), &n1).is_err());
}

#[test]
fn adversarial_tampered_ciphertext_is_rejected() {
	let store = store();
	let n1 = name("github");
	store.insert(&main_branch(), n1.clone(), sample_account("gh_pass_1"), add_change(&n1)).unwrap();

	let path = store.store_dir.join("branches/personal%3Amain/github.toml.age");
	let mut bytes = std::fs::read(&path).unwrap();
	let midpoint = bytes.len() / 2;
	bytes[midpoint] ^= 0b0101_1010;
	std::fs::write(&path, bytes).unwrap();

	assert!(store.get(&main_branch(), &n1).is_err());
}

#[test]
fn adversarial_failed_rekey_leaves_existing_ciphertext_readable() {
	#[derive(Clone)]
	struct FailingEncryption;

	impl EncryptionMethod for FailingEncryption {
		fn encrypt(&self, _plaintext: &[u8]) -> password::Result<Vec<u8>> {
			Err(Error::Encryption("simulated write-side failure".into()))
		}

		fn decrypt(&self, _ciphertext: &[u8]) -> password::Result<Vec<u8>> {
			Err(Error::Decryption("not used in this test".into()))
		}

		fn file_extension(&self) -> &'static str { "toml.age" }
	}

	let tmp = tempfile::tempdir().unwrap();
	let store = PijulStore::open(tmp.path().to_path_buf())
		.unwrap()
		.unlock_with(AgeScrypt::new("test-passphrase").unwrap());
	let n1 = name("github");
	store.insert(&main_branch(), n1.clone(), sample_account("gh_pass_1"), add_change(&n1)).unwrap();
	let path = store.store_dir.clone();
	let entry_path = path.join("branches/personal%3Amain/github.toml.age");
	let before = std::fs::read(&entry_path).unwrap();

	assert!(
		store.rekey_with(&main_branch(), FailingEncryption, rekey_change(&[n1.clone()])).is_err()
	);

	let after = std::fs::read(&entry_path).unwrap();
	assert_eq!(before, after);

	let recovered =
		PijulStore::open(path).unwrap().unlock_with(AgeScrypt::new("test-passphrase").unwrap());
	if let Item::OnlineAccount(account) = recovered.get(&main_branch(), &n1).unwrap().unwrap() {
		assert_eq!(account.password.as_deref(), Some("gh_pass_1"));
	}
}

#[test]
fn group_branch_paths_are_escaped_and_parent_access_is_inherited() {
	let store = store();
	let alice = PrincipalId::new("alice").unwrap();
	let parent = group_branch(["engineering"]);
	let child = group_branch(["engineering", "platform", "secrets"]);

	let mut acl = InMemoryAccessControl::default();
	acl.grant_branch(&alice, &parent, keyhive_core::access::Access::Edit);

	let child_edit = acl.authorize_branch::<GroupBranch, EditAccess>(&alice, &child).unwrap();
	let github = name("github");
	store
		.insert_authorized(
			&child_edit,
			github.clone(),
			sample_account("nested_group"),
			add_change(&github),
		)
		.unwrap();

	let storage_dir = store.store_dir.join("branches").join(child.storage_component());
	assert!(storage_dir.exists());
	assert!(!store.store_dir.join("branches/group:engineering/platform/secrets").exists());

	let child_read = acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &child).unwrap();
	assert_eq!(store.list_authorized(&child_read).unwrap(), vec![name("github")]);
}

#[test]
fn item_grants_allow_single_item_without_branch_membership() {
	let store = store();
	let owner = PrincipalId::new("owner").unwrap();
	let bob = PrincipalId::new("bob").unwrap();
	let branch = group_branch(["shared", "team"]);
	let github = name("github");
	let bank = name("bank");

	let mut acl = InMemoryAccessControl::default();
	acl.grant_branch(&owner, &branch, keyhive_core::access::Access::Edit);
	let owner_edit = acl.authorize_branch::<GroupBranch, EditAccess>(&owner, &branch).unwrap();
	store
		.insert_authorized(
			&owner_edit,
			github.clone(),
			sample_account("github_shared"),
			add_change(&github),
		)
		.unwrap();
	store
		.insert_authorized(&owner_edit, bank.clone(), sample_account("bank_private"), add_change(&bank))
		.unwrap();

	let github_target = ItemTarget::new(branch.clone(), github.clone());
	acl.grant_item(&bob, &github_target, keyhive_core::access::Access::Read);

	let bob_github = acl.authorize_item::<GroupBranch, ReadAccess>(&bob, &github_target).unwrap();
	assert!(store.get_authorized(&bob_github).unwrap().is_some());
	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&bob, &branch).is_err());
	assert!(
		acl.authorize_item::<GroupBranch, ReadAccess>(&bob, &ItemTarget::new(branch, bank)).is_err()
	);
}

#[test]
fn personal_branches_do_not_inherit_from_similar_slash_names() {
	let alice = PrincipalId::new("alice").unwrap();
	let personal = personal_branch("alice");
	let group = group_branch(["alice", "private"]);
	let mut acl = InMemoryAccessControl::default();
	acl.grant_branch(&alice, &personal, keyhive_core::access::Access::Admin);

	assert!(acl.authorize_branch::<password::PersonalBranch, ReadAccess>(&alice, &personal).is_ok());
	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &group).is_err());
}

#[test]
fn adversarial_branch_path_traversal_and_empty_segments_are_rejected() {
	for bad in ["../prod", "prod/../root", "prod//root", "prod/./root", "prod\\root", ""] {
		assert!(BranchSegment::new(bad).is_err(), "{bad:?} should be rejected");
	}
	for bad_segments in
		[vec!["prod", "..", "root"], vec!["prod", "", "root"], vec!["prod", ".", "root"], vec![
			"prod",
			"root/escape",
		]] {
		assert!(
			bad_segments.iter().copied().map(BranchSegment::new).any(|segment| segment.is_err()),
			"{bad_segments:?} should be rejected"
		);
	}
	for bad in ["../alice", "alice/bob", "alice\\bob", ".", "..", ""] {
		assert!(BranchSegment::new(bad).is_err(), "{bad:?} should be rejected");
	}
	assert!(BranchPath::<GroupBranch>::group(Vec::<BranchSegment>::new()).is_err());
}

#[test]
fn adversarial_branch_names_cannot_escape_or_collide_on_disk() {
	let store = store();
	let principal = PrincipalId::new("operator").unwrap();
	let mut acl = InMemoryAccessControl::default();
	let encoded_slash = group_branch(["ops%prod"]);
	let literal_slash = group_branch(["ops", "prod"]);

	assert_ne!(encoded_slash.storage_component(), literal_slash.storage_component());
	assert_eq!(encoded_slash.storage_component(), branch_storage_component(&encoded_slash));
	assert_eq!(literal_slash.storage_component(), branch_storage_component(&literal_slash));

	acl.grant_branch(&principal, &encoded_slash, keyhive_core::access::Access::Edit);
	acl.grant_branch(&principal, &literal_slash, keyhive_core::access::Access::Edit);

	let encoded_auth =
		acl.authorize_branch::<GroupBranch, EditAccess>(&principal, &encoded_slash).unwrap();
	let literal_auth =
		acl.authorize_branch::<GroupBranch, EditAccess>(&principal, &literal_slash).unwrap();
	let github = name("github");
	store
		.insert_authorized(
			&encoded_auth,
			github.clone(),
			sample_account("encoded"),
			add_change(&github),
		)
		.unwrap();
	store
		.insert_authorized(
			&literal_auth,
			github.clone(),
			sample_account("literal"),
			add_change(&github),
		)
		.unwrap();

	let branch_root = store.store_dir.join("branches");
	assert!(branch_root.join(encoded_slash.storage_component()).join("github.toml.age").exists());
	assert!(branch_root.join(literal_slash.storage_component()).join("github.toml.age").exists());
	assert!(!branch_root.join("group:ops").exists());

	if let Item::OnlineAccount(account) = store.get(&encoded_slash, &name("github")).unwrap().unwrap()
	{
		assert_eq!(account.password.as_deref(), Some("encoded"));
	}
	if let Item::OnlineAccount(account) = store.get(&literal_slash, &name("github")).unwrap().unwrap()
	{
		assert_eq!(account.password.as_deref(), Some("literal"));
	}
}

#[test]
fn branch_segment_newtype_rejects_unsafe_components_before_branch_construction() {
	assert!(BranchSegment::new("engineering").is_ok());
	for bad in ["", ".", "..", "ops/prod", "ops\\prod", "ops..prod", "ops%2Fprod", "ops%00prod"] {
		assert!(BranchSegment::new(bad).is_err(), "{bad:?} should be rejected");
	}
	assert!(serde_json::from_str::<BranchSegment>("\"ops/prod\"").is_err());
	assert!(serde_json::from_str::<BranchPath<PersonalBranch>>("\"personal:alice/bob\"").is_err());
	assert!(serde_json::from_str::<BranchPath<GroupBranch>>("\"group:ops/prod\"").is_ok());
}

#[test]
fn adversarial_read_item_grant_cannot_write_or_list_branch() {
	let store = store();
	let owner = PrincipalId::new("owner").unwrap();
	let attacker = PrincipalId::new("attacker").unwrap();
	let branch = group_branch(["finance", "payroll"]);
	let github = name("github");
	let github_target = ItemTarget::new(branch.clone(), github.clone());
	let mut acl = InMemoryAccessControl::default();

	acl.grant_branch(&owner, &branch, keyhive_core::access::Access::Edit);
	let owner_edit = acl.authorize_branch::<GroupBranch, EditAccess>(&owner, &branch).unwrap();
	store
		.insert_authorized(
			&owner_edit,
			github.clone(),
			sample_account("read_only"),
			add_change(&github),
		)
		.unwrap();

	acl.grant_item(&attacker, &github_target, keyhive_core::access::Access::Read);
	assert!(acl.authorize_item::<GroupBranch, EditAccess>(&attacker, &github_target).is_err());
	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&attacker, &branch).is_err());

	let read_token =
		acl.authorize_item::<GroupBranch, ReadAccess>(&attacker, &github_target).unwrap();
	assert!(store.get_authorized(&read_token).unwrap().is_some());
}

#[test]
fn adversarial_group_inheritance_stops_at_siblings_and_deeper_read_cannot_upgrade() {
	let store = store();
	let alice = PrincipalId::new("alice").unwrap();
	let platform = group_branch(["engineering", "platform"]);
	let secrets = group_branch(["engineering", "platform", "secrets"]);
	let sibling = group_branch(["engineering", "product", "secrets"]);
	let mut acl = InMemoryAccessControl::default();

	acl.grant_branch(&alice, &platform, keyhive_core::access::Access::Read);

	assert!(acl.authorize_branch::<GroupBranch, EditAccess>(&alice, &secrets).is_err());
	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &sibling).is_err());

	let owner = PrincipalId::new("owner").unwrap();
	acl.grant_branch(&owner, &secrets, keyhive_core::access::Access::Edit);
	let owner_edit = acl.authorize_branch::<GroupBranch, EditAccess>(&owner, &secrets).unwrap();
	let deploy = name("deploy");
	store
		.insert_authorized(&owner_edit, deploy.clone(), sample_account("secret"), add_change(&deploy))
		.unwrap();

	let inherited_read = acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &secrets).unwrap();
	assert_eq!(store.list_authorized(&inherited_read).unwrap(), vec![name("deploy")]);
}

#[test]
fn downstream_group_access_updates_after_parent_grant_is_added() {
	let alice = PrincipalId::new("alice").unwrap();
	let parent = group_branch(["ops"]);
	let grandchild = group_branch(["ops", "prod", "database"]);
	let mut acl = InMemoryAccessControl::default();

	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &grandchild).is_err());

	acl.grant_branch(&alice, &parent, keyhive_core::access::Access::Read);

	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &grandchild).is_ok());
	assert!(acl.authorize_branch::<GroupBranch, EditAccess>(&alice, &grandchild).is_err());
}

#[test]
fn downstream_group_access_updates_after_parent_permission_change_and_revoke() {
	let alice = PrincipalId::new("alice").unwrap();
	let parent = group_branch(["ops"]);
	let child = group_branch(["ops", "prod"]);
	let mut acl = InMemoryAccessControl::default();

	acl.grant_branch(&alice, &parent, keyhive_core::access::Access::Edit);
	assert!(acl.authorize_branch::<GroupBranch, EditAccess>(&alice, &child).is_ok());

	acl.grant_branch(&alice, &parent, keyhive_core::access::Access::Read);
	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &child).is_ok());
	assert!(acl.authorize_branch::<GroupBranch, EditAccess>(&alice, &child).is_err());

	acl.revoke_branch(&alice, &parent);
	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &child).is_err());
}

#[test]
fn downstream_group_direct_child_permission_overrides_parent_inheritance() {
	let alice = PrincipalId::new("alice").unwrap();
	let parent = group_branch(["engineering"]);
	let child = group_branch(["engineering", "payroll"]);
	let grandchild = group_branch(["engineering", "payroll", "taxes"]);
	let mut acl = InMemoryAccessControl::default();

	acl.grant_branch(&alice, &parent, keyhive_core::access::Access::Admin);
	acl.grant_branch(&alice, &child, keyhive_core::access::Access::Read);

	assert!(acl.authorize_branch::<GroupBranch, ReadAccess>(&alice, &grandchild).is_ok());
	assert!(acl.authorize_branch::<GroupBranch, EditAccess>(&alice, &grandchild).is_err());
}

#[test]
fn item_permission_changes_take_effect_for_downstream_group_items() {
	let bob = PrincipalId::new("bob").unwrap();
	let branch = group_branch(["shared", "deep", "team"]);
	let item = ItemTarget::new(branch, name("github"));
	let mut acl = InMemoryAccessControl::default();

	acl.grant_item(&bob, &item, keyhive_core::access::Access::Read);
	assert!(acl.authorize_item::<GroupBranch, ReadAccess>(&bob, &item).is_ok());
	assert!(acl.authorize_item::<GroupBranch, EditAccess>(&bob, &item).is_err());

	acl.grant_item(&bob, &item, keyhive_core::access::Access::Edit);
	assert!(acl.authorize_item::<GroupBranch, EditAccess>(&bob, &item).is_ok());

	acl.revoke_item(&bob, &item);
	assert!(acl.authorize_item::<GroupBranch, ReadAccess>(&bob, &item).is_err());
}

#[test]
fn policy_epoch_invalidates_offline_sessions_after_permission_changes_without_clock_time() {
	let alice = PrincipalId::new("alice").unwrap();
	let branch = group_branch(["ops"]);
	let mut acl = InMemoryAccessControl::default();
	let policy = password::OfflineSessionPolicy::ignored();

	acl.grant_branch(&alice, &branch, keyhive_core::access::Access::Read);
	let session = policy.issue("alice", acl.policy_epoch(), 1);
	assert!(policy.require_fresh(&session, acl.policy_epoch(), u64::MAX).is_ok());

	acl.grant_branch(&alice, &branch, keyhive_core::access::Access::Edit);
	assert!(policy.require_fresh(&session, acl.policy_epoch(), 1).is_err());

	let new_session = policy.issue("alice", acl.policy_epoch(), 2);
	assert!(policy.require_fresh(&new_session, acl.policy_epoch(), 0).is_ok());
}
