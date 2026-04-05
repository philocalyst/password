use std::{fs::{read, write}, io, path::PathBuf, time::Duration};
use color_eyre::eyre::{Context, Result};
use crossterm::event::{self, Event, KeyCode};
use ratatui::{prelude::*, widgets::{ListState, Block, Borders}};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use walkdir::WalkDir;

use crate::models::{PasswordStore, Item, OnlineAccount, AccountStatus, AuthProvider};
use crate::p2p::SyncState;
use crate::ui::{ItemList, ItemDetailView, FocusableField, render_sync_overlay};
use crate::app::commands::{SyncCommand, SyncResult};

pub mod commands;

pub enum Components {
	List,
	Content,
}

pub struct App {
	pub should_quit:          bool,
	pub store:                PasswordStore,
	pub focused:              Components,
	pub list_state:           ListState,
	pub detail_focused_field: Option<FocusableField>,
	// P2P Sync state
	pub sync_state:           SyncState,
	pub sync_sx:              mpsc::Sender<SyncCommand>,
	pub sync_rx:              mpsc::Receiver<SyncResult>,
	pub background_handle:    JoinHandle<anyhow::Result<()>>,
}

impl App {
	pub fn new(
		sync_sx: mpsc::Sender<SyncCommand>,
		sync_rx: mpsc::Receiver<SyncResult>,
		background_handle: JoinHandle<anyhow::Result<()>>
	) -> Self {
		let mut list = ListState::default();
		list.select(Some(0usize));
		let store = load_from_store(PathBuf::from("./store")).unwrap();

		Self {
			should_quit: false,
			focused: Components::List,
			store,
			list_state: list,
			detail_focused_field: None,
			sync_state: SyncState::Idle,
			sync_sx,
			sync_rx,
			background_handle
		}
	}

	pub fn get_current_item(&self) -> &Item {
		let selected_idx = self.list_state.selected().unwrap();

		// Sort the keys to match the list order
		let mut sorted_keys: Vec<&String> = self.store.items.keys().collect();
		sorted_keys.sort();

		// Get the key at the selected index
		let key = sorted_keys[selected_idx];

		// Return the item
		&self.store.items[key]
	}

	/// Get the first focusable field for the current item
	pub fn get_first_field_for_current_item(&self) -> Option<FocusableField> {
		let item = self.get_current_item();
		let detail_view = ItemDetailView { item, focused_field: None };
		let fields = detail_view.get_available_fields();
		fields.first().copied()
	}

	pub fn focus_next_field(&mut self) {
		let item = self.get_current_item();
		let mut detail_view = ItemDetailView { item, focused_field: self.detail_focused_field };
		detail_view.focus_next();
		self.detail_focused_field = detail_view.focused_field;
	}

	pub fn focus_prev_field(&mut self) {
		let item = self.get_current_item();
		let mut detail_view = ItemDetailView { item, focused_field: self.detail_focused_field };
		detail_view.focus_prev();
		self.detail_focused_field = detail_view.focused_field;
	}

	// Serialize the password store to bytes for P2P transfer
	pub fn serialize_store(&self) -> Result<Vec<u8>> {
		Ok(toml::to_string(&self.store)?.into_bytes())
	}

	// Deserialize a password store from bytes received via P2P
	pub fn deserialize_store(&mut self, data: &[u8]) -> Result<()> {
		let received: PasswordStore = toml::from_str(std::str::from_utf8(data)?)?;
		// Merge each received item into the current store
		for (key, item) in received.items {
			self.store.items.insert(key, item);
		}
		Ok(())
	}

	// Save the current store to disk
	pub fn save_store(&self) -> Result<()> {
		for (name, item) in &self.store.items {
			let path = PathBuf::from("./store").join(format!("{}.acc.toml", name));
			match item {
				Item::OnlineAccount(account) => {
					let content = toml::to_string_pretty(account)?;
					write(&path, content)?;
				}
				Item::SocialSecurity(ssn) => {
					let content = toml::to_string_pretty(ssn)?;
					write(&path, content)?;
				}
			}
		}
		Ok(())
	}

	// Start sharing passwords via P2P
	pub fn start_sharing(&mut self) {
		if let Ok(data) = self.serialize_store() {
			let _ = self.sync_sx.try_send(SyncCommand::Share(data));
			self.sync_state = SyncState::Sharing { ticket: "Generating ticket".to_string() };
		}
	}

	// Start receiving passwords via P2P
	pub fn start_receiving(&mut self) {
		self.sync_state = SyncState::ReceiveInput { input: String::new() };
	}

	// Cancel current sync operation
	pub fn cancel_sync(&mut self) {
		let _ = self.sync_sx.try_send(SyncCommand::Cancel);
		self.sync_state = SyncState::Idle;
	}

	// Check for sync results from background task
	pub fn poll_sync_results(&mut self) {
		while let Ok(result) = self.sync_rx.try_recv() {
			match result {
				SyncResult::TicketGenerated(ticket) => {
					self.sync_state = SyncState::Sharing { ticket };
				}
				SyncResult::DataReceived(data) => {
					match self.deserialize_store(&data) {
						Ok(_) => {
							let _ = self.save_store();
							self.sync_state = SyncState::Completed {
								message: "Passwords received and saved!".to_string(),
							};
						}
						Err(e) => {
							self.sync_state = SyncState::Error {
								message: format!("Failed to parse data: {}", e),
							};
						}
					}
				}
				SyncResult::Error(msg) => {
					self.sync_state = SyncState::Error { message: msg };
				}
			}
		}
	}

	// Run the main event loop until `should_quit` becomes true.
	pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
		const TICK_RATE: Duration = Duration::from_millis(100);

		while !self.should_quit {
			// Poll for sync results from background task
			self.poll_sync_results();

			terminal.draw(|f| self.render(f)).context("failed to draw frame")?;

			// Handle input with timeout
			if event::poll(TICK_RATE).context("failed to poll events")?
				&& let Event::Key(key_event) =
					event::read().context("failed to read event from terminal")?
				{
					self.handle_key(key_event);
				}
		}

		Ok(())
	}

	// Render the frame each loop iteration.
	pub fn render(&self, frame: &mut Frame) {
		let layout = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref());

		// Apply layout to the full terminal area
		let [area1, area2] = layout.split(frame.area())[..] else {
			panic!("Expected 2 layout regions");
		};

		// A simple frame for our display
		let _block = Block::default().title("Ratatui Example").borders(Borders::ALL);

		let item_list = ItemList(&self.store.items);

		let list: ratatui::widgets::List = item_list.clone().into();

		// Get the list state (Not possible for an unselect to occur)
		let selected_item_idx = self.list_state.selected().unwrap();

		// Determine the item to render (Should always associate with item)
		let selected_item = item_list.get_by_index(selected_item_idx).unwrap().1;

		// Pass a snapshot of the state at the time to render
		frame.render_stateful_widget(list, area1, &mut self.list_state.clone());

		// fixing the bg
		let border_south = area1.rows().next_back().unwrap_or_default();
		for xy in border_south.positions() {
			if let Some(c) = frame.buffer_mut().cell_mut(xy) {
				let style = c.style();
				c.set_style(style.bg(Color::Black));
			}
		}

		ItemDetailView { item: selected_item, focused_field: self.detail_focused_field }
			.render(frame, area2);

		// Render sync overlay if active
		render_sync_overlay(&self.sync_state, frame);
	}

	pub fn copy_field(&self) -> Result<()> {
		use arboard;

		let mut clipboard = arboard::Clipboard::new()?;

		clipboard.set_text(self.get_focused_field_value().unwrap())?;

		Ok(())
	}

	pub fn get_focused_field_value(&self) -> Option<String> {
		let item = self.get_current_item();
		let focused = self.detail_focused_field?;

		match item {
			Item::OnlineAccount(account) => match focused {
				FocusableField::Username => account.username.clone(),
				FocusableField::Email => account.email.as_ref().map(|e| e.to_string()),
				FocusableField::Phone => account.phone.as_ref().map(|p| p.to_string()),
				FocusableField::Password => account.password.clone(),
				FocusableField::Website => account.host_website.as_ref().map(|w| w.to_string()),
				FocusableField::Status => account.status.as_ref().map(|s| match s {
					AccountStatus::Active => "Active".to_string(),
					AccountStatus::Deactivated => "Deactivated".to_string(),
				}),
				FocusableField::TwoFactor => account.two_factor_enabled.map(|enabled| enabled.to_string()),
				FocusableField::SignInProviders => account.sign_in_with.as_ref().map(|providers| {
					providers
						.iter()
						.map(|p| match p {
							AuthProvider::Google => "Google",
							AuthProvider::Apple => "Apple",
							AuthProvider::Facebook => "Facebook",
						})
						.collect::<Vec<_>>()
						.join(", ")
				}),
				FocusableField::DateCreated => account.date_created.as_ref().map(|d| d.to_string()),
				FocusableField::SecurityQuestions => account.security_questions.as_ref().map(|qs| {
					qs.iter()
						.map(|q| format!("Q: {} A: {}", q.question, q.answer))
						.collect::<Vec<_>>()
						.join("
")
				}),
				FocusableField::Notes => account.notes.clone(),
				_ => None,
			},
			Item::SocialSecurity(ssn) => match focused {
				FocusableField::AccountNumber => Some(ssn.account_number.clone()),
				FocusableField::LegalName => ssn.legal_name.as_ref().map(|n| n.display_full().to_string()),
				FocusableField::Country => ssn.country_of_issue.as_ref().map(|c| c.to_string()),
				FocusableField::IssuanceDate => ssn.issuance_date.as_ref().map(|d| d.to_string()),
				_ => None,
			},
		}
	}

	pub fn handle_key(&mut self, key_event: event::KeyEvent) {
		// Handle sync modal input first if active
		if !matches!(self.sync_state, SyncState::Idle) {
			self.handle_sync_key(key_event);
			return;
		}

		match self.focused {
			Components::List => {
				match key_event.code {
					KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
					KeyCode::Down | KeyCode::Char('j') => self.cycle_forward(),
					KeyCode::Up | KeyCode::Char('k') => self.cycle_backward(),
					KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
						self.focused = Components::Content;
						// Initialize with first field
						self.detail_focused_field = self.get_first_field_for_current_item();
					}
					// P2P Sync shortcuts
					KeyCode::Char('s') => self.start_sharing(),
					KeyCode::Char('r') => self.start_receiving(),
					_ => {}
				}
			}
			Components::Content => match key_event.code {
				KeyCode::Char('q') => self.should_quit = true,
				KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => {
					self.focused = Components::List;
					self.detail_focused_field = None;
				}
				KeyCode::Down | KeyCode::Char('j') => self.focus_next_field(),
				KeyCode::Char(' ') => self.copy_field().unwrap(),
				KeyCode::Up | KeyCode::Char('k') => self.focus_prev_field(),
				// P2P Sync shortcuts
				KeyCode::Char('s') => self.start_sharing(),
				KeyCode::Char('r') => self.start_receiving(),
				_ => {}
			},
		}
	}

	/// Handle key events when sync modal is active
	pub fn handle_sync_key(&mut self, key_event: event::KeyEvent) {
		match &mut self.sync_state {
			SyncState::Sharing { .. } => {
				if matches!(key_event.code, KeyCode::Esc) {
					self.cancel_sync();
				}
			}
			SyncState::ReceiveInput { input } => {
				match key_event.code {
					KeyCode::Esc => {
						self.sync_state = SyncState::Idle;
					}
					KeyCode::Enter => {
						if !input.is_empty() {
							let ticket = input.clone();
							let _ = self.sync_sx.try_send(SyncCommand::Receive(ticket));
							self.sync_state = SyncState::Receiving;
						}
					}
					KeyCode::Backspace => {
						input.pop();
					}
					KeyCode::Char('v') if key_event.modifiers.contains(event::KeyModifiers::CONTROL) => {
						// Paste from clipboard
						if let Ok(mut clipboard) = arboard::Clipboard::new() {
							if let Ok(text) = clipboard.get_text() {
								input.push_str(&text);
							}
						}
					}
					KeyCode::Char(c) => {
						input.push(c);
					}
					_ => {}
				}
			}
			SyncState::Receiving => {
				// Can't cancel during receive, just wait
			}
			SyncState::Completed { .. } | SyncState::Error { .. } => {
				if matches!(key_event.code, KeyCode::Enter | KeyCode::Esc) {
					self.sync_state = SyncState::Idle;
				}
			}
			SyncState::Idle => {}
		}
	}

	pub fn cycle_forward(&mut self) {
		let current_position = self.list_state.selected().unwrap();

		if self.store.items.len() - 1 == current_position {
			self.list_state.select_first();
		} else {
			self.list_state.select_next();
		}
	}

	pub fn cycle_backward(&mut self) {
		let current_position = self.list_state.selected().unwrap();

		if 0 == current_position {
			self.list_state.select(Some(self.store.items.len() - 1));
		} else {
			self.list_state.select_previous();
		}
	}
}

pub fn load_from_store(store_path: PathBuf) -> Result<PasswordStore> {
	let mut items: std::collections::HashMap<String, Item> = std::collections::HashMap::default();

	for entry in WalkDir::new(store_path) {
		let entry = entry?;

		if entry.path().is_dir() {
			continue;
		}

		let file_bytes: Vec<u8> = read(entry.path())?;

		// The item before deriving associated items and certain attributes using the
		// filename
		let raw_item: OnlineAccount = toml::from_slice(&file_bytes)?;

		// Derive the account, which is practically just the filename
		let identification = entry
			.path()
			.file_name()
			.unwrap()
			.to_string_lossy()
			.split_once(".")
			.unwrap()
			.0
			.to_string();

		items.insert(identification, Item::OnlineAccount(raw_item));
	}

	Ok(PasswordStore { items })
}
