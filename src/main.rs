use std::{path::PathBuf, str::FromStr as _};

use clap::{Parser, Subcommand};
use password::{AccountName, AgeScrypt, BranchPath, BranchSegment, Item, PersonalBranch, PijulStore, ShareTicket, StoreBackend, StoreChange, VersionedEntry, models::{AccountStatus, OnlineAccount, SocialSecurity}, p2p::{IrohSyncHandle, decode_store, encode_store}};

/// A type-safe, Pijul-versioned, Iroh P2P credential store.
#[derive(Parser)]
#[command(name = "pwd", about, version)]
struct Cli {
	/// Path to the store directory (defaults to $HOME/.pwd).
	#[arg(long, short = 'd', global = true, env = "PWD_STORE_DIR")]
	store_dir: Option<PathBuf>,

	/// The branch (party) to operate on.
	#[arg(long, short = 'b', global = true, default_value = "main")]
	branch: String,

	/// Store passphrase. Defaults to $PWD_STORE_PASSPHRASE or an interactive
	/// prompt.
	#[arg(long, global = true, env = "PWD_STORE_PASSPHRASE")]
	passphrase: Option<String>,

	#[command(subcommand)]
	command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
	/// Initialize the password store branch (automatically created if not
	/// present, but can be done explicitly).
	Init,

	/// Add a new credential entry.
	Add {
		/// Unique name for this entry.
		name: String,

		/// Entry type: "online" (default) or "ssn".
		#[arg(long, default_value = "online")]
		r#type: String,

		/// Password (online accounts only).
		#[arg(long)]
		password: Option<String>,

		/// Username.
		#[arg(long)]
		username: Option<String>,

		/// E-mail address.
		#[arg(long)]
		email: Option<String>,

		/// Host website URL.
		#[arg(long)]
		website: Option<String>,

		/// Record message for history.
		#[arg(long, short = 'm', default_value = "add entry")]
		message: String,
	},

	/// Print a credential entry to stdout.
	Get {
		/// Entry name.
		name: String,

		/// Print only this field (e.g. "password", "username").
		#[arg(long)]
		field: Option<String>,
	},

	/// Remove a credential entry.
	Remove {
		/// Entry name.
		name: String,

		/// Record message for history.
		#[arg(long, short = 'm', default_value = "remove entry")]
		message: String,
	},

	/// List all credential entries.
	List,

	/// Show the change history.
	Log {
		/// Show history for one entry only.
		#[arg(long, short = 'e')]
		entry: Option<String>,
	},

	/// Show the content of an entry at a specific patch hash.
	Show {
		/// Entry name.
		name: String,

		/// Patch hash (base32) to restore to.
		#[arg(long)]
		at: String,
	},

	/// Revert an entry to the state it had after a specific patch.
	Revert {
		/// Entry name.
		name: String,

		/// Target patch hash (base32).
		#[arg(long)]
		to: String,
	},

	/// Show a unified diff of an entry between two patches.
	Diff {
		/// Entry name.
		name: String,

		/// Patch hash to diff from.
		#[arg(long)]
		from: String,

		/// Patch hash to diff to (omit to diff against current state).
		#[arg(long)]
		to: Option<String>,
	},

	/// Share the store branch via Iroh; prints a ticket for the receiver.
	Share,

	/// Receive a shared store branch via an Iroh ticket.
	Receive {
		/// Ticket string printed by `pwd share`.
		ticket: String,
	},

	/// Re-encrypt this branch with a new passphrase.
	Rekey {
		/// New store passphrase. Defaults to $PWD_STORE_NEW_PASSPHRASE or an
		/// interactive prompt.
		#[arg(long, env = "PWD_STORE_NEW_PASSPHRASE")]
		new_passphrase: Option<String>,

		/// Record message for history.
		#[arg(long, short = 'm', default_value = "rekey store")]
		message: String,
	},
}

// ── entry point

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let cli = Cli::parse();

	let store_dir = cli
		.store_dir
		.unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".pwd"));

	let locked_store = PijulStore::open(&store_dir)?;
	let branch = personal_branch(&cli.branch)?;

	match cli.command {
		Cmd::Init => {
			locked_store.init(&branch)?;
			println!("Initialized branch '{branch}' in {}", store_dir.display());
		}

		Cmd::Add { name, r#type, password, username, email, website, message } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let account_name = AccountName::new(&name)?;
			let item = match r#type.as_str() {
				"ssn" => Item::SocialSecurity(SocialSecurity {
					account_number:   name.clone().parse().unwrap_or_else(|_| unimplemented!()),
					legal_name:       None,
					issuance_date:    None,
					country_of_issue: None,
					notes:            None,
				}),
				_ => {
					let host_website = website.as_deref().map(|u| u.parse::<url::Url>()).transpose()?;
					let email_addr =
						email.as_deref().map(|e| e.parse::<email_address::EmailAddress>()).transpose()?;

					Item::OnlineAccount(OnlineAccount {
						username,
						password,
						email: email_addr,
						phone: None,
						sign_in_with: None,
						status: Some(AccountStatus::Active),
						host_website,
						login_pages: None,
						security_questions: None,
						date_created: Some(jiff::Zoned::now().date()),
						two_factor_enabled: None,
						associated_items: None,
						notes: None,
					})
				}
			};

			store.insert(&branch, account_name.clone(), item, StoreChange::Custom(message))?;
			println!("Added '{name}' to branch '{branch}'");
		}

		Cmd::Get { name, field } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let account_name = AccountName::new(&name)?;
			match store.get(&branch, &account_name)? {
				None => eprintln!("No entry '{name}' on branch '{branch}'"),
				Some(item) => {
					if let Some(f) = field {
						println!("{}", extract_field(&item, &f).unwrap_or_default());
					} else {
						println!("{}", toml::to_string_pretty(&item)?);
					}
				}
			}
		}

		Cmd::Remove { name, message } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let account_name = AccountName::new(&name)?;
			let removed = store.remove(&branch, &account_name, StoreChange::Custom(message))?;
			if removed {
				println!("Removed '{name}' from branch '{branch}'");
			} else {
				eprintln!("No entry '{name}' found on branch '{branch}'");
			}
		}

		Cmd::List => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let names = store.list(&branch)?;
			if names.is_empty() {
				println!("(empty store on branch '{branch}')");
			} else {
				for n in names {
					println!("{n}");
				}
			}
		}

		Cmd::Log { entry } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let filter = match entry {
				Some(ref n) => Some(AccountName::new(n)?),
				None => None,
			};
			let entries = store.log_impl(&branch, filter.as_ref())?;
			if entries.is_empty() {
				println!("(no history on branch '{branch}')");
			}
			for e in entries {
				let scope = e.entry_name.as_ref().map(|n| format!(" [{}]", n)).unwrap_or_default();
				println!("{}  {}  {}{scope}", e.hash, e.timestamp, e.message);
			}
		}

		Cmd::Show { name, at } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let account_name = AccountName::new(&name)?;
			use pijul_at_core::Base32;
			let hash = pijul_at_core::Hash::from_base32(at.as_bytes())
				.ok_or_else(|| anyhow::anyhow!("invalid hash: {at}"))?;

			match store.entry(&branch, account_name).snapshot_at(&hash)? {
				Some(item) => println!("{}", toml::to_string_pretty(&item)?),
				None => eprintln!("Entry '{name}' not found at patch {at}"),
			}
		}

		Cmd::Revert { name, to } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let account_name = AccountName::new(&name)?;
			use pijul_at_core::Base32;
			let hash = pijul_at_core::Hash::from_base32(to.as_bytes())
				.ok_or_else(|| anyhow::anyhow!("invalid hash: {to}"))?;

			store.entry(&branch, account_name).revert_to(&hash)?;
			println!("Reverted '{name}' to {to} on branch '{branch}'");
		}

		Cmd::Diff { name, from, to } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let account_name = AccountName::new(&name)?;
			use pijul_at_core::Base32;
			let from_hash = pijul_at_core::Hash::from_base32(from.as_bytes())
				.ok_or_else(|| anyhow::anyhow!("invalid hash: {from}"))?;
			let to_hash = to
				.as_deref()
				.map(|h| {
					pijul_at_core::Hash::from_base32(h.as_bytes())
						.ok_or_else(|| anyhow::anyhow!("invalid 'to' hash"))
				})
				.transpose()?;

			let diff = store.entry(&branch, account_name).diff(&from_hash, to_hash.as_ref())?;

			println!("diff for {}", diff.label);
			for line in diff.lines {
				use password::store::DiffOp;
				match line.op {
					DiffOp::Retain => println!("  {}", line.content),
					DiffOp::Insert => println!("+ {}", line.content),
					DiffOp::Delete => println!("- {}", line.content),
				}
			}
		}

		Cmd::Share => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let loaded = store.load(&branch)?;
			let payload = encode_store(&loaded)?;

			let handle = IrohSyncHandle::new();
			let ticket = handle.share(payload).await?;
			println!("{ticket}");

			// Wait for termination.
			tokio::signal::ctrl_c().await?;
			handle.shutdown().await?;
		}

		Cmd::Receive { ticket } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let share_ticket = ShareTicket::from_str(&ticket)?;
			let handle = IrohSyncHandle::new();
			let payload = handle.receive(&share_ticket).await?;
			handle.shutdown().await?;

			let received = decode_store(payload)?;
			let mut current = store.load(&branch)?;
			for (name, item) in received.items {
				current.items.insert(name, item);
			}
			store.save(&branch, &current)?;
			println!("Store updated on branch '{branch}' — {} entries now.", current.items.len());
		}

		Cmd::Rekey { new_passphrase, message } => {
			let store = unlock_store(locked_store, cli.passphrase)?;
			let new_passphrase = read_passphrase(new_passphrase, "New store passphrase")?;
			let _store =
				store.rekey_with(&branch, AgeScrypt::new(new_passphrase)?, StoreChange::Custom(message))?;
			println!("Rekeyed branch '{branch}' in {}", store_dir.display());
		}
	}

	Ok(())
}

fn unlock_store(
	store: PijulStore,
	passphrase: Option<String>,
) -> anyhow::Result<password::versioning::PijulStore<password::Unlocked<AgeScrypt>>> {
	Ok(store.unlock_with(AgeScrypt::new(read_passphrase(passphrase, "Store passphrase")?)?))
}

fn read_passphrase(passphrase: Option<String>, prompt: &str) -> anyhow::Result<String> {
	match passphrase {
		Some(passphrase) => Ok(passphrase),
		None => Ok(rpassword::prompt_password(format!("{prompt}: "))?),
	}
}

fn personal_branch(raw: &str) -> anyhow::Result<BranchPath<PersonalBranch>> {
	Ok(BranchPath::personal(BranchSegment::new(raw)?))
}

fn extract_field(item: &Item, field: &str) -> Option<String> {
	match item {
		Item::OnlineAccount(a) => match field {
			"username" => a.username.clone(),
			"password" => a.password.clone(),
			"email" => a.email.as_ref().map(|e| e.to_string()),
			"phone" => a.phone.as_ref().map(|p| p.to_string()),
			"website" => a.host_website.as_ref().map(|u| u.to_string()),
			"2fa" => a.two_factor_enabled.map(|b| b.to_string()),
			"status" => a.status.as_ref().map(|s| format!("{s:?}")),
			"notes" => a.notes.clone(),
			_ => None,
		},
		Item::SocialSecurity(s) => match field {
			"number" | "account_number" => Some(s.account_number.to_string()),
			"name" | "legal_name" => s.legal_name.as_ref().map(|n| n.to_string()),
			"country" | "country_of_issue" => s.country_of_issue.as_ref().map(|c| c.to_string()),
			"issued" | "issuance_date" => s.issuance_date.as_ref().map(|d| d.to_string()),
			"notes" => s.notes.clone(),
			_ => None,
		},
	}
}
