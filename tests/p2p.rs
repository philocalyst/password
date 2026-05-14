//! Integration tests for the Iroh P2P transport.
//!
//! Spins up two in-process IrohSync nodes and verifies payload sync across
//! peers via tickets, rather than just unit-testing serialization.

use std::time::Duration;

use password::{AccountName, Item, PasswordStore, models::{AccountStatus, OnlineAccount}, p2p::{IrohSyncHandle, decode_store, encode_store}};
use tokio::time::timeout;

fn generate_store(name: &str, num_items: usize) -> PasswordStore {
	let mut s = PasswordStore::new();
	for i in 0..num_items {
		let n = AccountName::new(format!("{name}-{i}")).unwrap();
		let item = Item::OnlineAccount(OnlineAccount {
			username:           Some(format!("user-{i}")),
			password:           Some("secret".into()),
			email:              None,
			phone:              None,
			sign_in_with:       None,
			status:             Some(AccountStatus::Active),
			host_website:       None,
			login_pages:        None,
			security_questions: None,
			date_created:       None,
			two_factor_enabled: None,
			associated_items:   None,
			notes:              None,
		});
		s.items.insert(n, item);
	}
	s
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_functions_between_concurrent_peers() {
	let original = generate_store("work-account", 5);
	let payload = encode_store(&original).expect("encode");

	// Sender node
	let sender = IrohSyncHandle::new();

	// Share payload asynchronously
	let ticket = sender.share(payload).await.expect("share");

	// Receiver node runs concurrently
	let receiver_task = tokio::spawn(async move {
		let receiver = IrohSyncHandle::new();
		let received_payload = timeout(
			Duration::from_secs(10), // Wait reasonable time for discovery/gossip
			receiver.receive(&ticket),
		)
		.await
		.expect("receive timed out")
		.expect("receive payload");

		receiver.shutdown().await.ok();
		received_payload
	});

	let received_payload = receiver_task.await.expect("task failed");
	let received = decode_store(received_payload).expect("decode");

	assert_eq!(received.items.len(), original.items.len(), "item count across sync must match");

	// Verify content integrity
	for (name, orig_item) in original.items.iter() {
		let rec_item = received.items.get(name).expect("missing synced item");
		if let (Item::OnlineAccount(o), Item::OnlineAccount(r)) = (orig_item, rec_item) {
			assert_eq!(o.username, r.username, "payload data was mangled over sync");
		}
	}

	sender.shutdown().await.ok();
}

#[tokio::test]
async fn invalid_tickets_are_rejected_gracefully() {
	let receiver = IrohSyncHandle::new();
	let bad_ticket = password::ShareTicket("docaaqaamaax3x3x3x3x3...garbage".into());

	let res = receiver.receive(&bad_ticket).await;
	assert!(res.is_err(), "Invalid tickets must be rejected before attempting sync");

	receiver.shutdown().await.ok();
}
