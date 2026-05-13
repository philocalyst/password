//! Per-entry Pijul-backed versioning with multi-branch support.

use std::{collections::BTreeMap, path::PathBuf};

use jiff::Timestamp;
use pijul_at_core::{ArcTxn, Base32, ChannelRef, Hash, MutTxnT, MutTxnTExt, TxnT, TxnTExt, change::{Author, ChangeHeader}, changestore::ChangeStore, working_copy::filesystem::FileSystem};
use pijul_at_repository::Repository;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::{Error, Result, models::{AccountName, Item, PasswordStore}, store::{DiffLine, DiffOp, DiffResult, StoreBackend, VersionedEntry}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEntry {
	pub hash:       String,
	pub message:    String,
	pub timestamp:  Timestamp,
	pub author:     String,
	pub entry_name: Option<AccountName>,
}

pub struct PijulStore {
	pub store_dir: PathBuf,
	repo:          Repository,
	_temp:         Option<TempDir>,
}

impl PijulStore {
	pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
		let store_dir: PathBuf = path.into();
		std::fs::create_dir_all(&store_dir)?;
		std::fs::create_dir_all(store_dir.join("branches"))?;

		let repo = if store_dir.join(pijul_at_core::DOT_DIR).exists() {
			Repository::find_root(Some(store_dir.as_path()))
				.map_err(|e| Error::RepoInit(e.to_string()))?
		} else {
			let config =
				pijul_at_config::Config::load(None, vec![]).map_err(|e| Error::RepoInit(e.to_string()))?;
			let repo = Repository::init(&config, Some(store_dir.as_path()), None, None)
				.map_err(|e| Error::RepoInit(e.to_string()))?;
			let mut txn = repo.pristine.mut_txn_begin().map_err(|e| Error::Pijul(e.to_string()))?;
			txn.open_or_create_channel("main").map_err(|e| Error::Pijul(e.to_string()))?;
			txn.set_current_channel("main").map_err(|e| Error::Pijul(e.to_string()))?;
			txn.commit().map_err(|e| Error::Pijul(e.to_string()))?;
			std::fs::create_dir_all(store_dir.join("branches").join("main"))?;
			repo
		};

		Ok(Self { store_dir, repo, _temp: None })
	}

	pub fn ephemeral() -> Result<Self> {
		let tmp = tempfile::tempdir()?;
		let mut store = Self::open(tmp.path().to_path_buf())?;
		store._temp = Some(tmp);
		Ok(store)
	}

	fn branch_dir(&self, branch: &str) -> PathBuf { self.store_dir.join("branches").join(branch) }

	fn entry_path(&self, branch: &str, name: &AccountName) -> PathBuf {
		self.branch_dir(branch).join(format!("{}.toml", name.as_str()))
	}

	fn write_entry(&self, branch: &str, name: &AccountName, item: &Item) -> Result<()> {
		let toml = toml::to_string_pretty(item)?;
		std::fs::create_dir_all(self.branch_dir(branch))?;
		std::fs::write(self.entry_path(branch, name), toml)?;
		Ok(())
	}

	fn remove_entry_file(&self, branch: &str, name: &AccountName) -> Result<bool> {
		let path = self.entry_path(branch, name);
		if path.exists() {
			std::fs::remove_file(&path)?;
			Ok(true)
		} else {
			Ok(false)
		}
	}

	fn get_or_create_channel<T: pijul_at_core::MutTxnTExt>(
		txn: &mut T,
		name: &str,
	) -> Result<ChannelRef<T>> {
		txn.open_or_create_channel(name).map_err(|e| Error::Pijul(e.to_string()))
	}

	fn pijul_record(&self, branch: &str, name: &AccountName, msg: &str, added: bool) -> Result<Hash> {
		self.init(branch)?;
		let txn = self.repo.pristine.arc_txn_begin().map_err(|e| Error::Pijul(e.to_string()))?;

		let channel = {
			let mut txn_w = txn.write();
			Self::get_or_create_channel(&mut *txn_w, branch)?
		};

		let rel_path = format!("branches/{}/{}.toml", branch, name.as_str());
		{
			let mut txn_w = txn.write();
			if added {
				if !txn_w.is_tracked(&rel_path).map_err(|e| Error::Pijul(e.to_string()))? {
					let salt = rand::random::<u64>();
					txn_w.add_file(&rel_path, salt).map_err(|e| Error::Pijul(format!("{e:?}")))?;
				}
			} else {
				if txn_w.is_tracked(&rel_path).map_err(|e| Error::Pijul(e.to_string()))? {
					txn_w.remove_file(&rel_path).map_err(|e| Error::Pijul(format!("{e:?}")))?;
				}
			}
		}

		txn
			.write()
			.apply_root_change_if_needed(&self.repo.changes, &channel, &mut rand::rng())
			.map_err(|e| Error::Pijul(e.to_string()))?;

		let mut state = pijul_at_core::RecordBuilder::new();
		let working_copy = FileSystem::from_root(self.store_dir.to_str().unwrap());
		state
			.record(
				txn.clone(),
				pijul_at_core::Algorithm::default(),
				false,
				&pijul_at_core::DEFAULT_SEPARATOR,
				channel.clone(),
				&working_copy,
				&self.repo.changes,
				&rel_path,
				1,
			)
			.map_err(|e| Error::Pijul(e.to_string()))?;

		let rec = state.finish();
		if rec.actions.is_empty() {
			return Err(Error::NothingToRecord);
		}

		let mut author_map = BTreeMap::new();
		author_map.insert("name".to_string(), "pwd".to_string());

		let header = ChangeHeader {
			message:     msg.to_owned(),
			authors:     vec![Author(author_map)],
			description: Some(format!("entry: {}", name.as_str())),
			timestamp:   jiff::Timestamp::now(),
		};

		{
			let txn_r = txn.read();
			let actions = rec.actions.into_iter().map(|a| a.globalize(&*txn_r).unwrap()).collect();
			let contents = std::sync::Arc::try_unwrap(rec.contents).unwrap().into_inner();
			let change = pijul_at_core::change::LocalChange::make_change(
				&*txn_r,
				&channel,
				actions,
				contents,
				header,
				vec![],
			)
			.map_err(|e| Error::Pijul(e.to_string()))?;
			drop(txn_r);

			let mut change = change;
			let hash = self
				.repo
				.changes
				.save_change(&mut change, |_, _| Ok::<_, anyhow::Error>(()))
				.map_err(|e| Error::Pijul(e.to_string()))?;

			txn
				.write()
				.apply_local_change(&mut { channel }, &change, &hash, &rec.updatables)
				.map_err(|e| Error::Pijul(e.to_string()))?;
			txn.commit().map_err(|e| Error::Pijul(e.to_string()))?;
			Ok(hash)
		}
	}

	fn find_hashes_to_unrecord_since<T: pijul_at_core::TxnTExt>(
		&self,
		txn: &ArcTxn<T>,
		channel: &ChannelRef<T>,
		branch: &str,
		name: &AccountName,
		target: &Hash,
	) -> Result<Vec<Hash>> {
		let rel_path = format!("branches/{}/{}.toml", branch, name.as_str());
		let g = txn.read();
		let mut rev_iter =
			g.reverse_log(&channel.read(), None).map_err(|e| Error::Pijul(e.to_string()))?;
		let mut out = Vec::new();

		loop {
			match rev_iter.next() {
				Some(Ok((_, (hash, _)))) => {
					let h: Hash = hash.into();
					if &h == target {
						break;
					}
					let touched = g.touched_files(&h).map_err(|e| Error::Pijul(e.to_string()))?;
					if let Some(mut iter) = touched {
						if iter.any(|r| {
							r.ok()
								.and_then(|pos| {
									g.find_oldest_path(&self.repo.changes, channel, &pos).ok().flatten()
								})
								.map(|(p, _)| p == rel_path)
								.unwrap_or(false)
						}) {
							out.push(h);
						}
					}
				}
				Some(Err(e)) => return Err(Error::Pijul(e.to_string())),
				None => return Err(Error::PatchNotFound { hash: target.to_base32() }),
			}
		}
		Ok(out)
	}
}

impl StoreBackend for PijulStore {
	type Error = Error;

	fn init(&self, branch: &str) -> Result<()> {
		std::fs::create_dir_all(self.branch_dir(branch))?;
		let txn = self.repo.pristine.arc_txn_begin().map_err(|e| Error::Pijul(e.to_string()))?;
		{
			let mut txn_w = txn.write();
			Self::get_or_create_channel(&mut *txn_w, branch)?;
		}
		txn.commit().map_err(|e| Error::Pijul(e.to_string()))?;
		Ok(())
	}

	fn load(&self, branch: &str) -> Result<PasswordStore> {
		let mut store = PasswordStore::new();
		let dir = self.branch_dir(branch);
		if !dir.exists() {
			return Ok(store);
		}
		for entry in std::fs::read_dir(dir)? {
			let entry = entry?;
			let path = entry.path();
			if path.extension().and_then(|e| e.to_str()) != Some("toml") {
				continue;
			}
			let stem = path.file_stem().unwrap().to_str().unwrap().to_owned();
			let name = match AccountName::new(stem) {
				Ok(n) => n,
				Err(_) => continue,
			};
			let content = std::fs::read_to_string(&path)?;
			let item: Item = match toml::from_str(&content) {
				Ok(i) => i,
				Err(_) => continue,
			};
			store.items.insert(name, item);
		}
		Ok(store)
	}

	fn save(&self, branch: &str, store: &PasswordStore) -> Result<()> {
		self.init(branch)?;
		for (name, item) in &store.items {
			self.write_entry(branch, name, item)?;
		}
		Ok(())
	}

	fn list(&self, branch: &str) -> Result<Vec<AccountName>> {
		let mut names: Vec<AccountName> = self.load(branch)?.items.keys().cloned().collect();
		names.sort();
		Ok(names)
	}

	fn get(&self, branch: &str, name: &AccountName) -> Result<Option<Item>> {
		Ok(self.load(branch)?.items.get(name).cloned())
	}

	fn insert(&self, branch: &str, name: AccountName, item: Item, msg: &str) -> Result<()> {
		if self.get(branch, &name)?.is_some() {
			return Err(Error::EntryAlreadyExists { name });
		}
		self.write_entry(branch, &name, &item)?;
		let _ = self.pijul_record(branch, &name, msg, true);
		Ok(())
	}

	fn update(&self, branch: &str, name: &AccountName, item: Item, msg: &str) -> Result<()> {
		if self.get(branch, name)?.is_none() {
			return Err(Error::EntryNotFound { name: name.clone() });
		}
		self.write_entry(branch, name, &item)?;
		let _ = self.pijul_record(branch, name, msg, true);
		Ok(())
	}

	fn remove(&self, branch: &str, name: &AccountName, msg: &str) -> Result<bool> {
		let existed = self.get(branch, name)?.is_some();
		let removed_file = self.remove_entry_file(branch, name)?;
		if existed || removed_file {
			let _ = self.pijul_record(branch, name, msg, false);
		}
		Ok(existed || removed_file)
	}
}

// ── Entry handle ──────────────────────────────────────────────────────────────

/// A borrowed reference to a specific entry in a branch.
///
/// Returned by [`PijulStore::entry`]; provides per-entry version operations
/// without repeating the branch or name on every call.
pub struct EntryHandle<'s> {
	store:  &'s PijulStore,
	branch: String,
	name:   AccountName,
}

impl PijulStore {
	pub fn entry(&self, branch: impl Into<String>, name: AccountName) -> EntryHandle<'_> {
		EntryHandle { store: self, branch: branch.into(), name }
	}
}

impl VersionedEntry for EntryHandle<'_> {
	type Error = Error;

	fn revert_to(&self, target: &Hash) -> Result<()> {
		self.store.revert_entry_impl(&self.branch, &self.name, target)
	}

	fn log(&self) -> Result<Vec<ChangeEntry>> { self.store.log_entry(&self.branch, &self.name) }

	fn diff(&self, from: &Hash, to: Option<&Hash>) -> Result<DiffResult> {
		self.store.diff_entry_impl(&self.branch, &self.name, from, to)
	}

	fn snapshot_at(&self, at: &Hash) -> Result<Option<Item>> {
		self.store.snapshot_entry_at(&self.branch, &self.name, at)
	}

	fn head(&self) -> Result<Option<Hash>> { self.store.head_entry_hash(&self.branch, &self.name) }
}

struct DiffSink<'a> {
	before: &'a [&'a str],
	after:  &'a [&'a str],
	lines:  Vec<DiffLine>,
}

impl<'a> imara_diff::sink::Sink for DiffSink<'a> {
	type Out = Vec<DiffLine>;

	fn process_change(&mut self, before_pos: std::ops::Range<u32>, after_pos: std::ops::Range<u32>) {
		for range in before_pos {
			self.lines.push(DiffLine {
				op:      DiffOp::Delete,
				content: self.before[range as usize].to_owned(),
			});
		}
		for range in after_pos {
			self
				.lines
				.push(DiffLine { op: DiffOp::Insert, content: self.after[range as usize].to_owned() });
		}
	}

	fn finish(self) -> <DiffSink<'a> as imara_diff::sink::Sink>::Out { self.lines }
}

impl PijulStore {
	/// Revert `name` to the state it had after `target`. Used by `EntryHandle`.
	pub fn revert_entry_impl(&self, branch: &str, name: &AccountName, target: &Hash) -> Result<()> {
		let txn = self.repo.pristine.arc_txn_begin().map_err(|e| Error::Pijul(e.to_string()))?;
		let channel = {
			let mut txn_w = txn.write();
			Self::get_or_create_channel(&mut *txn_w, branch)?
		};
		let hashes_to_unrecord =
			self.find_hashes_to_unrecord_since(&txn, &channel, branch, name, target)?;
		let working_copy = FileSystem::from_root(self.store_dir.to_str().unwrap());

		{
			let mut txn_w = txn.write();
			for h in &hashes_to_unrecord {
				txn_w
					.unrecord(&self.repo.changes, &channel, h, rand::random::<u64>(), &working_copy)
					.map_err(|e| Error::RevertConflict(e.to_string()))?;
			}
		}

		pijul_at_core::output::output_repository_no_pending(
			&working_copy,
			&self.repo.changes,
			&txn,
			&channel,
			"",
			true,
			None,
			1,
			rand::random::<u64>(),
		)
		.map_err(|e| Error::Pijul(e.to_string()))?;

		txn.commit().map_err(|e| Error::Pijul(e.to_string()))?;
		Ok(())
	}

	/// Scoped log for a single entry. Used by `EntryHandle`.
	pub fn log_entry(&self, branch: &str, name: &AccountName) -> Result<Vec<ChangeEntry>> {
		self.log_impl(branch, Some(name))
	}

	/// Full or filtered branch log. Used by the FFI and CLI.
	pub fn log_impl(&self, branch: &str, filter: Option<&AccountName>) -> Result<Vec<ChangeEntry>> {
		let txn = self.repo.pristine.txn_begin().map_err(|e| Error::Pijul(e.to_string()))?;
		let channel_ref = match txn.load_channel(branch).map_err(|e| Error::Pijul(e.to_string()))? {
			Some(c) => c,
			None => return Ok(vec![]),
		};

		let filter_str = filter.map(|n| format!("entry: {}", n.as_str()));
		let mut entries = Vec::new();

		for item in
			txn.reverse_log(&channel_ref.read(), None).map_err(|e| Error::Pijul(e.to_string()))?
		{
			let (_, (hash_ser, _)) = item.map_err(|e| Error::Pijul(e.to_string()))?;
			let hash: Hash = hash_ser.into();
			let header = self.repo.changes.get_header(&hash).map_err(|e| Error::Pijul(e.to_string()))?;

			if let Some(ref f) = filter_str {
				if header.description.as_deref() != Some(f.as_str()) {
					continue;
				}
			}

			let entry_name = header
				.description
				.as_deref()
				.and_then(|d| d.strip_prefix("entry: "))
				.and_then(|n| AccountName::new(n).ok());

			let author = header
				.authors
				.first()
				.and_then(|a| a.0.get("name").or_else(|| a.0.get("key")))
				.cloned()
				.unwrap_or_else(|| "unknown".into());

			entries.push(ChangeEntry {
				hash: hash.to_base32(),
				message: header.message,
				timestamp: header.timestamp,
				author,
				entry_name,
			});
		}
		Ok(entries)
	}

	pub fn diff_entry_impl(
		&self,
		branch: &str,
		name: &AccountName,
		from: &Hash,
		to: Option<&Hash>,
	) -> Result<DiffResult> {
		let content_at = |hash: &Hash| -> Result<String> {
			let header = self.repo.changes.get_header(hash).map_err(|e| Error::Pijul(e.to_string()))?;
			Ok(format!(
				"hash: {}\nmessage: {}\ntimestamp: {}",
				hash.to_base32(),
				header.message,
				header.timestamp
			))
		};
		let from_text = content_at(from)?;
		let to_text = match to {
			Some(h) => content_at(h)?,
			None => {
				let p = self.entry_path(branch, name);
				if p.exists() { std::fs::read_to_string(&p)? } else { String::new() }
			}
		};

		let before: Vec<&str> = from_text.lines().collect();
		let after: Vec<&str> = to_text.lines().collect();

		let input = imara_diff::intern::InternedInput::new(from_text.as_str(), to_text.as_str());
		let sink = DiffSink { before: &before, after: &after, lines: Vec::new() };
		let tokens = imara_diff::diff(imara_diff::Algorithm::Histogram, &input, sink);

		Ok(DiffResult { label: format!("{branch}/{name}"), lines: tokens })
	}

	pub fn snapshot_entry_at(&self, branch: &str, name: &AccountName, at: &Hash) -> Result<Option<Item>> {
		let header = self.repo.changes.get_header(at).map_err(|e| Error::Pijul(e.to_string()))?;
		if header.description.as_deref().map(|d| d.contains(name.as_str())) != Some(true) {
			return Ok(None);
		}
		Ok(self.get(branch, name)?)
	}

	pub fn head_entry_hash(&self, branch: &str, name: &AccountName) -> Result<Option<Hash>> {
		let entries = self.log_entry(branch, name)?;
		Ok(
			entries
				.into_iter()
				.next()
				.and_then(|e| Hash::from_base32(e.hash.as_bytes()).map(|h| h.into())),
		)
	}
}
