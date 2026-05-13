//! Integration tests for `StoreBackend` + `Versioned` via `PijulStore`.
//! Testing complex versioning, branching, and reverting scenarios.

use password::{AccountName, Item, PijulStore, StoreBackend, Versioned};
use password::models::{OnlineAccount, AccountStatus};

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

fn name(s: &str) -> AccountName {
    AccountName::new(s).unwrap()
}

#[test]
fn branches_are_isolated() {
    let store = PijulStore::ephemeral().unwrap();
    let n1 = name("github");
    
    // Insert into 'alice' branch
    store.insert("alice", n1.clone(), sample_account("alice_pass")).unwrap();
    store.record_entry("alice", &n1, "add").unwrap();

    // Insert into 'bob' branch
    store.insert("bob", n1.clone(), sample_account("bob_pass")).unwrap();
    store.record_entry("bob", &n1, "add").unwrap();

    // Verify isolation
    if let Item::OnlineAccount(a) = store.get("alice", &n1).unwrap().unwrap() {
        assert_eq!(a.password.as_deref(), Some("alice_pass"));
    }
    if let Item::OnlineAccount(b) = store.get("bob", &n1).unwrap().unwrap() {
        assert_eq!(b.password.as_deref(), Some("bob_pass"));
    }
}

#[test]
fn reverting_entry_preserves_other_entries_across_interleaved_history() {
    let store = PijulStore::ephemeral().unwrap();
    let n1 = name("github");
    let n2 = name("bank");

    // v1: github created
    store.insert("main", n1.clone(), sample_account("gh_pass_1")).unwrap();
    let hash_gh_1 = store.record_entry("main", &n1, "add github").unwrap();

    // v2: bank created
    store.insert("main", n2.clone(), sample_account("bank_pass")).unwrap();
    let _hash_bank = store.record_entry("main", &n2, "add bank").unwrap();

    // v3: github updated
    store.update("main", &n1, sample_account("gh_pass_2")).unwrap();
    let _hash_gh_2 = store.record_entry("main", &n1, "update github").unwrap();

    // Now we want to revert 'github' back to hash_gh_1.
    // This MUST NOT affect 'bank' which was added in between (hash_bank).
    store.revert_entry("main", &n1, &hash_gh_1).unwrap();

    // Verify github is reverted
    if let Item::OnlineAccount(a) = store.get("main", &n1).unwrap().unwrap() {
        assert_eq!(a.password.as_deref(), Some("gh_pass_1"));
    }

    // Verify bank is untouched
    if let Item::OnlineAccount(a) = store.get("main", &n2).unwrap().unwrap() {
        assert_eq!(a.password.as_deref(), Some("bank_pass"));
    }
}
