//! Per-entry Pijul-backed versioning with multi-branch support.

use std::{collections::BTreeMap, path::PathBuf};

use jiff::Timestamp;
use pijul_at_core::{ArcTxn, Base32, ChannelRef, Hash, MutTxnT, MutTxnTExt, TxnT, TxnTExt, change::{Author, ChangeHeader}, changestore::ChangeStore, working_copy::filesystem::FileSystem};
use pijul_at_repository::Repository;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::{Error, Result, access_control::{self, Authorized, BranchKind, BranchPath, BranchTarget, GrantsEdit, GrantsRead, ItemTarget}, encryption::{EncryptionMethod, Locked, Unlocked}, models::{AccountName, Item, PasswordStore}, store::{DiffLine, DiffOp, DiffResult, StoreBackend, StoreChange, VersionedEntry}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEntry {
	pub hash:       String,
	pub message:    String,
	pub timestamp:  Timestamp,
	pub author:     String,
	pub entry_name: Option<AccountName>,
}

pub struct PijulStore<State = Locked> {
	pub store_dir: PathBuf,
	repo:          Repository,
	_temp:         Option<TempDir>,
	state:         State,
}

impl PijulStore<Locked> {
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

		Ok(Self { store_dir, repo, _temp: None, state: Locked })
	}

	pub fn ephemeral() -> Result<Self> {
		let tmp = tempfile::tempdir()?;
		let mut store = Self::open(tmp.path().to_path_buf())?;
		store._temp = Some(tmp);
		Ok(store)
	}

	pub fn unlock_with<M: EncryptionMethod>(self, method: M) -> PijulStore<Unlocked<M>> {
		PijulStore {
			store_dir: self.store_dir,
			repo:      self.repo,
			_temp:     self._temp,
			state:     Unlocked::new(method),
		}
	}
}

impl<State> PijulStore<State> {
	fn branch_dir(&self, branch: &str) -> PathBuf {
		self.store_dir.join("branches").join(access_control::branch_storage_component_raw(branch))
	}

	pub fn init<K: BranchKind>(&self, branch: &BranchPath<K>) -> Result<()> {
		self.init_raw(branch.as_str())
	}

	fn init_raw(&self, branch: &str) -> Result<()> {
		std::fs::create_dir_all(self.branch_dir(branch))?;
		let txn = self.repo.pristine.arc_txn_begin().map_err(|e| Error::Pijul(e.to_string()))?;
		{
			let mut txn_w = txn.write();
			Self::get_or_create_channel(&mut *txn_w, branch)?;
		}
		txn.commit().map_err(|e| Error::Pijul(e.to_string()))?;
		Ok(())
	}

	fn get_or_create_channel<T: pijul_at_core::MutTxnTExt>(
		txn: &mut T,
		name: &str,
	) -> Result<ChannelRef<T>> {
		txn.open_or_create_channel(name).map_err(|e| Error::Pijul(e.to_string()))
	}
}

impl<M: EncryptionMethod> PijulStore<Unlocked<M>> {
	fn entry_path(&self, branch: &str, name: &AccountName) -> PathBuf {
		self.branch_dir(branch).join(format!(
			"{}.{}",
			name.as_str(),
			self.state.method.file_extension()
		))
	}

	pub fn list_authorized<K: BranchKind, L: GrantsRead>(
		&self,
		branch: &Authorized<'_, BranchTarget<K>, L>,
	) -> Result<Vec<AccountName>> {
		self.list(branch.branch())
	}

	pub fn get_authorized<K: BranchKind, L: GrantsRead>(
		&self,
		item: &Authorized<'_, ItemTarget<K>, L>,
	) -> Result<Option<Item>> {
		self.get(item.branch(), item.name())
	}

	pub fn insert_authorized<K: BranchKind, L: GrantsEdit>(
		&self,
		branch: &Authorized<'_, BranchTarget<K>, L>,
		name: AccountName,
		item: Item,
		change: StoreChange,
	) -> Result<()> {
		self.insert(branch.branch(), name, item, change)
	}

	pub fn update_authorized<K: BranchKind, L: GrantsEdit>(
		&self,
		item_auth: &Authorized<'_, ItemTarget<K>, L>,
		item: Item,
		change: StoreChange,
	) -> Result<()> {
		self.update(item_auth.branch(), item_auth.name(), item, change)
	}

	pub fn remove_authorized<K: BranchKind, L: GrantsEdit>(
		&self,
		item_auth: &Authorized<'_, ItemTarget<K>, L>,
		change: StoreChange,
	) -> Result<bool> {
		self.remove(item_auth.branch(), item_auth.name(), change)
	}

	fn write_entry(&self, branch: &str, name: &AccountName, item: &Item) -> Result<()> {
		let toml = toml::to_string_pretty(item)?;
		let encrypted = self.state.method.encrypt(toml.as_bytes())?;
		let path = self.entry_path(branch, name);
		let dir = self.branch_dir(branch);
		std::fs::create_dir_all(&dir)?;
		let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
		std::io::Write::write_all(&mut tmp, &encrypted)?;
		tmp.persist(&path).map_err(|e| e.error)?;
		Ok(())
	}

	fn read_entry(&self, path: &std::path::Path) -> Result<Item> {
		let encrypted = std::fs::read(path)?;
		let plaintext = self.state.method.decrypt(&encrypted)?;
		let content = std::str::from_utf8(&plaintext)?;
		Ok(toml::from_str(content)?)
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

	pub fn rekey_with<K: BranchKind, N: EncryptionMethod>(
		self,
		branch: &BranchPath<K>,
		new_method: N,
		change: StoreChange,
	) -> Result<PijulStore<Unlocked<N>>> {
		let branch_name = branch.as_str();
		let current = self.load(branch)?;
		let store = PijulStore {
			store_dir: self.store_dir,
			repo:      self.repo,
			_temp:     self._temp,
			state:     Unlocked::new(new_method),
		};
		store.save(branch, &current)?;
		for name in current.items.keys() {
			let msg = change.message();
			let _ = store.pijul_record(branch_name, name, &msg, true);
		}
		Ok(store)
	}

	fn pijul_record(&self, branch: &str, name: &AccountName, msg: &str, added: bool) -> Result<Hash> {
		self.init_raw(branch)?;
		let txn = self.repo.pristine.arc_txn_begin().map_err(|e| Error::Pijul(e.to_string()))?;

		let channel = {
			let mut txn_w = txn.write();
			Self::get_or_create_channel(&mut *txn_w, branch)?
		};

		let branch_component = access_control::branch_storage_component_raw(branch);
		let rel_path = format!(
			"branches/{}/{}.{}",
			branch_component,
			name.as_str(),
			self.state.method.file_extension()
		);
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
		let branch_component = access_control::branch_storage_component_raw(branch);
		let rel_path = format!(
			"branches/{}/{}.{}",
			branch_component,
			name.as_str(),
			self.state.method.file_extension()
		);
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

impl<M: EncryptionMethod> StoreBackend for PijulStore<Unlocked<M>> {
	type Error = Error;

	fn init<K: BranchKind>(&self, branch: &BranchPath<K>) -> Result<()> {
		PijulStore::init(self, branch)
	}

	fn load<K: BranchKind>(&self, branch: &BranchPath<K>) -> Result<PasswordStore> {
		let mut store = PasswordStore::new();
		let dir = self.branch_dir(branch.as_str());
		if !dir.exists() {
			return Ok(store);
		}
		for entry in std::fs::read_dir(dir)? {
			let entry = entry?;
			let path = entry.path();
			let filename = match path.file_name().and_then(|e| e.to_str()) {
				Some(filename) => filename,
				None => continue,
			};
			if !filename.ends_with(self.state.method.file_extension()) {
				continue;
			}
			let stem =
				filename.trim_end_matches(self.state.method.file_extension()).trim_end_matches('.');
			let name = match AccountName::new(stem) {
				Ok(n) => n,
				Err(_) => continue,
			};
			let item = self.read_entry(&path)?;
			store.items.insert(name, item);
		}
		Ok(store)
	}

	fn save<K: BranchKind>(&self, branch: &BranchPath<K>, store: &PasswordStore) -> Result<()> {
		self.init(branch)?;
		for (name, item) in &store.items {
			self.write_entry(branch.as_str(), name, item)?;
		}
		Ok(())
	}

	fn list<K: BranchKind>(&self, branch: &BranchPath<K>) -> Result<Vec<AccountName>> {
		let mut names: Vec<AccountName> = self.load(branch)?.items.keys().cloned().collect();
		names.sort();
		Ok(names)
	}

	fn get<K: BranchKind>(&self, branch: &BranchPath<K>, name: &AccountName) -> Result<Option<Item>> {
		Ok(self.load(branch)?.items.get(name).cloned())
	}

	fn insert<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: AccountName,
		item: Item,
		change: StoreChange,
	) -> Result<()> {
		validate_change_target(&change, &name)?;
		if self.get(branch, &name)?.is_some() {
			return Err(Error::EntryAlreadyExists { name });
		}
		self.write_entry(branch.as_str(), &name, &item)?;
		let msg = change.message();
		let _ = self.pijul_record(branch.as_str(), &name, &msg, true);
		Ok(())
	}

	fn update<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: &AccountName,
		item: Item,
		change: StoreChange,
	) -> Result<()> {
		validate_change_target(&change, name)?;
		if self.get(branch, name)?.is_none() {
			return Err(Error::EntryNotFound { name: name.clone() });
		}
		self.write_entry(branch.as_str(), name, &item)?;
		let msg = change.message();
		let _ = self.pijul_record(branch.as_str(), name, &msg, true);
		Ok(())
	}

	fn remove<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: &AccountName,
		change: StoreChange,
	) -> Result<bool> {
		validate_change_target(&change, name)?;
		let existed = self.get(branch, name)?.is_some();
		let removed_file = self.remove_entry_file(branch.as_str(), name)?;
		if existed || removed_file {
			let msg = change.message();
			let _ = self.pijul_record(branch.as_str(), name, &msg, false);
		}
		Ok(existed || removed_file)
	}
}

fn validate_change_target(change: &StoreChange, name: &AccountName) -> Result<()> {
	if let Some(change_name) = change.entry_name() {
		if change_name != name {
			return Err(Error::Validation {
				field:  "change.name".into(),
				reason: format!("change targets {change_name}, but operation targets {name}"),
			});
		}
	}
	Ok(())
}

// ── Entry handle
// ──────────────────────────────────────────────────────────────

/// A borrowed reference to a specific entry in a branch.
///
/// Returned by [`PijulStore::entry`]; provides per-entry version operations
/// without repeating the branch or name on every call.
pub struct EntryHandle<'s, M: EncryptionMethod> {
	store:  &'s PijulStore<Unlocked<M>>,
	branch: String,
	name:   AccountName,
}

impl<M: EncryptionMethod> PijulStore<Unlocked<M>> {
	pub fn entry<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		name: AccountName,
	) -> EntryHandle<'_, M> {
		EntryHandle { store: self, branch: branch.as_str().to_owned(), name }
	}
}

impl<M: EncryptionMethod> VersionedEntry for EntryHandle<'_, M> {
	type Error = Error;

	fn revert_to(&self, target: &Hash) -> Result<()> {
		self.store.revert_entry_impl_raw(&self.branch, &self.name, target)
	}

	fn log(&self) -> Result<Vec<ChangeEntry>> { self.store.log_entry_raw(&self.branch, &self.name) }

	fn diff(&self, from: &Hash, to: Option<&Hash>) -> Result<DiffResult> {
		self.store.diff_entry_impl_raw(&self.branch, &self.name, from, to)
	}

	fn snapshot_at(&self, at: &Hash) -> Result<Option<Item>> {
		self.store.snapshot_entry_at_raw(&self.branch, &self.name, at)
	}

	fn head(&self) -> Result<Option<Hash>> {
		self.store.head_entry_hash_raw(&self.branch, &self.name)
	}
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

impl<M: EncryptionMethod> PijulStore<Unlocked<M>> {
	/// Revert `name` to the state it had after `target`. Used by `EntryHandle`.
	fn revert_entry_impl_raw(&self, branch: &str, name: &AccountName, target: &Hash) -> Result<()> {
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
	fn log_entry_raw(&self, branch: &str, name: &AccountName) -> Result<Vec<ChangeEntry>> {
		self.log_impl_raw(branch, Some(name))
	}

	/// Full or filtered branch log. Used by the FFI and CLI.
	pub fn log_impl<K: BranchKind>(
		&self,
		branch: &BranchPath<K>,
		filter: Option<&AccountName>,
	) -> Result<Vec<ChangeEntry>> {
		self.log_impl_raw(branch.as_str(), filter)
	}

	fn log_impl_raw(&self, branch: &str, filter: Option<&AccountName>) -> Result<Vec<ChangeEntry>> {
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

	fn diff_entry_impl_raw(
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
				if p.exists() { toml::to_string_pretty(&self.read_entry(&p)?)? } else { String::new() }
			}
		};

		let before: Vec<&str> = from_text.lines().collect();
		let after: Vec<&str> = to_text.lines().collect();

		let input = imara_diff::intern::InternedInput::new(from_text.as_str(), to_text.as_str());
		let sink = DiffSink { before: &before, after: &after, lines: Vec::new() };
		let tokens = imara_diff::diff(imara_diff::Algorithm::Histogram, &input, sink);

		Ok(DiffResult { label: format!("{branch}/{name}"), lines: tokens })
	}

	fn snapshot_entry_at_raw(
		&self,
		branch: &str,
		name: &AccountName,
		at: &Hash,
	) -> Result<Option<Item>> {
		let header = self.repo.changes.get_header(at).map_err(|e| Error::Pijul(e.to_string()))?;
		if header.description.as_deref().map(|d| d.contains(name.as_str())) != Some(true) {
			return Ok(None);
		}
		let path = self.entry_path(branch, name);
		if path.exists() { Ok(Some(self.read_entry(&path)?)) } else { Ok(None) }
	}

	fn head_entry_hash_raw(&self, branch: &str, name: &AccountName) -> Result<Option<Hash>> {
		let entries = self.log_entry_raw(branch, name)?;
		Ok(
			entries
				.into_iter()
				.next()
				.and_then(|e| Hash::from_base32(e.hash.as_bytes()).map(|h| h.into())),
		)
	}
}
