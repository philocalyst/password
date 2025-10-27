use std::{collections::HashMap, fs::read, io::{self, stdout}, path::PathBuf, time::{Duration, Instant}};

use celes::Country;
use color_eyre::eyre::{Context, Result};
use crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode}, execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode}};
use email_address::EmailAddress;
use human_name::Name;
use jiff::civil::Date;
use phonenumber::PhoneNumber;
use ratatui::{layout::Rect, prelude::*, widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap}};
use serde::{Deserialize, de::DeserializeSeed};
use url::Url;
use walkdir::WalkDir;

fn deserialize_name<'de, D>(deserializer: D) -> Result<Option<Name>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s: Option<String> = Option::deserialize(deserializer)?;
	match s {
		Some(name_str) => Ok(Name::parse(&name_str)),
		None => Ok(None),
	}
}

/// Application state. Can be expanded later with UI data.
struct App<'a> {
	should_quit: bool,
	store:       PasswordStore<'a>,
	focused:     Components,
	list_state:  ListState,
}

enum Components {
	List,
	Content,
}

#[derive(Default)]
struct PasswordStore<'a> {
	items: HashMap<String, Item<'a>>,
}

#[derive(Deserialize)]
enum Item<'a> {
	OnlineAccount(OnlineAccount<'a>),
	SocialSecurity(SocialSecurity),
}

#[derive(Deserialize)]
struct SocialSecurity {
	account_number:   String,
	#[serde(deserialize_with = "deserialize_name")]
	legal_name:       Option<Name>,
	issuance_date:    Option<Date>,
	country_of_issue: Option<Country>,
}

#[derive(Deserialize)]
enum AuthProvider {
	Google,
	Apple,
	Facebook,
}

#[derive(Deserialize)]
struct OnlineAccount<'a> {
	#[serde(skip)]
	account:               String,
	username:              Option<String>,
	email:                 Option<EmailAddress>,
	phone:                 Option<PhoneNumber>,
	sign_in_with:          Option<Vec<AuthProvider>>,
	password:              Option<String>,
	status:                Option<AccountStatus>,
	host_website:          Option<Url>,
	login_pages:           Option<Vec<Url>>,
	security_questions:    Option<Vec<SecurityQuestion>>,
	date_created:          Option<Date>,
	two_factor_enabled:    Option<bool>,
	#[serde(default, rename = "associated_items")]
	associated_item_names: Vec<String>, // Temporarily store names
	#[serde(skip)]
	associated_items:      Vec<&'a Item<'a>>,
	notes:                 Option<String>,
}

impl<'a> OnlineAccount<'a> {
	fn resolve_associations(&mut self, item_map: &'a HashMap<String, Item<'a>>) {
		self.associated_items =
			self.associated_item_names.iter().filter_map(|name| item_map.get(name.as_str())).collect();
	}
}

#[derive(Deserialize)]
enum AccountStatus {
	Active,
	Deactivated,
}

#[derive(Deserialize)]
struct SecurityQuestion {
	question: String,
	answer:   String,
}

#[derive(Clone)]
struct ItemList<'a>(&'a HashMap<String, Item<'a>>);
struct ItemDetailView<'a>(&'a Item<'a>);

impl<'a> ItemDetailView<'a> {
	fn render(&self, frame: &mut Frame, area: Rect) {
		match self.0 {
			Item::OnlineAccount(account) => self.render_online_account(frame, area, account),
			Item::SocialSecurity(ssn) => self.render_social_security(frame, area, ssn),
		}
	}

	fn render_online_account(&self, frame: &mut Frame, area: Rect, account: &OnlineAccount) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(3), // Header
				Constraint::Min(10),   // Main content
				Constraint::Length(5), // Notes section
			])
			.split(area);

		// Header with account name
		let header = Paragraph::new(Line::from(vec![
			Span::styled("🔐 ", Style::default().fg(Color::Yellow)),
			Span::styled(&account.account, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
		]))
		.block(
			Block::default()
				.borders(Borders::ALL)
				.border_style(Style::default().fg(Color::Cyan))
				.title(" Online Account ")
				.title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
		);
		frame.render_widget(header, chunks[0]);

		// Main content area - split into two columns
		let content_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
			.split(chunks[1]);

		// Left column - Credentials
		let mut cred_lines = vec![Line::from(vec![
			Span::styled("Username: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
			Span::styled(account.username.as_deref().unwrap_or("N/A"), Style::default().fg(Color::White)),
		])];

		if let Some(email) = &account.email {
			cred_lines.push(Line::from(vec![
				Span::styled("Email: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
				Span::styled(email.to_string(), Style::default().fg(Color::White)),
			]));
		}

		if let Some(phone) = &account.phone {
			cred_lines.push(Line::from(vec![
				Span::styled("Phone: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
				Span::styled(phone.to_string(), Style::default().fg(Color::White)),
			]));
		}

		if let Some(password) = &account.password {
			let masked = "•".repeat(password.len().min(16));
			cred_lines.push(Line::from(vec![
				Span::styled("Password: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
				Span::styled(masked, Style::default().fg(Color::DarkGray)),
			]));
		}

		if let Some(website) = &account.host_website {
			cred_lines.push(Line::from(""));
			cred_lines.push(Line::from(vec![
				Span::styled("🌐 ", Style::default().fg(Color::Blue)),
				Span::styled(
					website.to_string(),
					Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
				),
			]));
		}

		let credentials = Paragraph::new(cred_lines)
			.block(
				Block::default()
					.borders(Borders::ALL)
					.border_style(Style::default().fg(Color::Green))
					.title(" Credentials ")
					.title_style(Style::default().fg(Color::Green)),
			)
			.wrap(Wrap { trim: true });
		frame.render_widget(credentials, content_chunks[0]);

		// Right column - Security & Status
		let mut security_lines = vec![];

		if let Some(status) = &account.status {
			let (status_text, status_color) = match status {
				AccountStatus::Active => ("● ACTIVE", Color::Green),
				AccountStatus::Deactivated => ("○ DEACTIVATED", Color::Red),
			};
			security_lines.push(Line::from(vec![
				Span::styled("Status: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
				Span::styled(status_text, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
			]));
		}

		if let Some(two_fa) = account.two_factor_enabled {
			let (icon, color) = if two_fa { ("✓", Color::Green) } else { ("✗", Color::Red) };
			security_lines.push(Line::from(vec![
				Span::styled("2FA: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
				Span::styled(icon, Style::default().fg(color).add_modifier(Modifier::BOLD)),
				Span::styled(if two_fa { " Enabled" } else { " Disabled" }, Style::default().fg(color)),
			]));
		}

		if let Some(providers) = &account.sign_in_with {
			if !providers.is_empty() {
				security_lines.push(Line::from(""));
				security_lines.push(Line::from(Span::styled(
					"Sign in with:",
					Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
				)));
				for provider in providers {
					let (icon, name, color) = match provider {
						AuthProvider::Google => ("G", "Google", Color::Red),
						AuthProvider::Apple => ("", "Apple", Color::White),
						AuthProvider::Facebook => ("f", "Facebook", Color::Blue),
					};
					security_lines.push(Line::from(vec![
						Span::raw("  "),
						Span::styled(icon, Style::default().fg(color).add_modifier(Modifier::BOLD)),
						Span::raw(" "),
						Span::styled(name, Style::default().fg(color)),
					]));
				}
			}
		}

		if let Some(date) = &account.date_created {
			security_lines.push(Line::from(""));
			security_lines.push(Line::from(vec![
				Span::styled("Created: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
				Span::styled(date.to_string(), Style::default().fg(Color::Gray)),
			]));
		}

		if let Some(questions) = &account.security_questions {
			security_lines.push(Line::from(""));
			security_lines.push(Line::from(Span::styled(
				format!("Security Questions: {}", questions.len()),
				Style::default().fg(Color::Yellow),
			)));
		}

		let security = Paragraph::new(security_lines)
			.block(
				Block::default()
					.borders(Borders::ALL)
					.border_style(Style::default().fg(Color::Magenta))
					.title(" Security ")
					.title_style(Style::default().fg(Color::Magenta)),
			)
			.wrap(Wrap { trim: true });
		frame.render_widget(security, content_chunks[1]);

		// Notes section
		if let Some(notes) = &account.notes {
			let notes_widget = Paragraph::new(notes.as_str())
				.block(
					Block::default()
						.borders(Borders::ALL)
						.border_style(Style::default().fg(Color::Yellow))
						.title(" 📝 Notes ")
						.title_style(Style::default().fg(Color::Yellow)),
				)
				.wrap(Wrap { trim: true })
				.style(Style::default().fg(Color::Gray));
			frame.render_widget(notes_widget, chunks[2]);
		}
	}

	fn render_social_security(&self, frame: &mut Frame, area: Rect, ssn: &SocialSecurity) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(3), // Header
				Constraint::Min(8),    // Content
			])
			.split(area);

		// Header
		let header = Paragraph::new(Line::from(vec![
			Span::styled("🛡️  ", Style::default().fg(Color::Red)),
			Span::styled(
				"Social Security Number",
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
			),
		]))
		.block(
			Block::default()
				.borders(Borders::ALL)
				.border_style(Style::default().fg(Color::Red))
				.title(" Sensitive Information ")
				.title_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
		);
		frame.render_widget(header, chunks[0]);

		// Content
		let mut lines = vec![Line::from(vec![
			Span::styled(
				"Account Number: ",
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
			),
			Span::styled(
				format!("***-**-{}", &ssn.account_number[ssn.account_number.len().saturating_sub(4)..]),
				Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
			),
		])];

		if let Some(name) = &ssn.legal_name {
			lines.push(Line::from(""));
			lines.push(Line::from(vec![
				Span::styled("Legal Name: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
				Span::styled(name.display_full(), Style::default().fg(Color::White)),
			]));
		}

		if let Some(country) = &ssn.country_of_issue {
			lines.push(Line::from(vec![
				Span::styled("Country: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
				Span::styled(country.to_string(), Style::default().fg(Color::White)),
			]));
		}

		if let Some(date) = &ssn.issuance_date {
			lines.push(Line::from(vec![
				Span::styled("Issued: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
				Span::styled(date.to_string(), Style::default().fg(Color::Gray)),
			]));
		}

		let content = Paragraph::new(lines)
			.block(
				Block::default()
					.borders(Borders::ALL)
					.border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
					.title(" ⚠️  CONFIDENTIAL ")
					.title_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
			)
			.wrap(Wrap { trim: true });
		frame.render_widget(content, chunks[1]);
	}
}

impl<'a> From<ItemList<'a>> for List<'a> {
	fn from(items: ItemList<'a>) -> Self {
		let mut sorted_items: Vec<(&String, &Item<'a>)> = items.0.iter().collect();
		sorted_items.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));

		let list_items: Vec<ListItem<'a>> = sorted_items
			.iter()
			.map(|(name, _)| {
				let line = Line::from(vec![
					Span::styled(name.to_string(), Style::default().fg(Color::Green)),
					Span::raw(" | "),
				]);
				ListItem::new(line)
			})
			.collect();
		List::new(list_items)
			.highlight_symbol("> ")
			.highlight_style(Style::default().add_modifier(Modifier::BOLD))
	}
}

impl<'a> ItemList<'a> {
	/// Get an item by its index in the alphabetically sorted list
	pub fn get_by_index(&self, index: usize) -> Option<(&String, &Item<'a>)> {
		let mut sorted_items: Vec<(&String, &Item<'a>)> = self.0.iter().collect();
		sorted_items.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
		sorted_items.get(index).copied()
	}
}

impl<'a> App<'a> {
	/// Create a new instance with default values.
	fn new() -> Self {
		// Define the default selected item (the first)
		let mut list = ListState::default();
		list.select(Some(0usize));

		let store = load_from_store(PathBuf::from("../store")).unwrap();

		Self { should_quit: false, focused: Components::List, store, list_state: list }
	}

	/// Run the main event loop until `should_quit` becomes true.
	fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
		const TICK_RATE: Duration = Duration::from_millis(1000);

		while !self.should_quit {
			terminal.draw(|f| self.render(f)).context("failed to draw frame")?;

			// Handle input with timeout
			if event::poll(TICK_RATE).context("failed to poll events")? {
				if let Event::Key(key_event) =
					event::read().context("failed to read event from terminal")?
				{
					self.handle_key(key_event);
				}
			}
		}

		Ok(())
	}

	/// Render the frame each loop iteration.
	fn render(&self, frame: &mut Frame) {
		let layout = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref());

		// Apply layout to the full terminal area
		let [area1, area2] = layout.split(frame.area())[..] else {
			panic!("Expected 2 layout regions");
		};

		// A simple frame for our display
		let block = Block::default().title("Ratatui Example").borders(Borders::ALL);

		let item_list = ItemList(&self.store.items);

		let list: List = item_list.clone().into();

		// Get the list state (Not possible for an unselect to occur)
		let selected_item_idx = self.list_state.selected().unwrap();

		// Determine the item to render (Should always associate with item)
		let selected_item = item_list.get_by_index(selected_item_idx).unwrap().1;

		// Pass a snapshot of the state at the time to render
		frame.render_stateful_widget(list, area1, &mut self.list_state.clone());
		ItemDetailView(selected_item).render(frame, area2);
	}

	/// Handle key input and update state.
	fn handle_key(&mut self, key_event: event::KeyEvent) {
		match key_event.code {
			KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
			KeyCode::Down | KeyCode::Char('j') => self.cycle_forward(),
			KeyCode::Up | KeyCode::Char('k') => self.cycle_backward(),
			_ => {}
		}
	}

	fn cycle_forward(&mut self) {
		let current_position = self.list_state.selected().unwrap();

		if self.store.items.len() - 1 == current_position {
			self.list_state.select_first();
		} else {
			self.list_state.select_next();
		}
	}

	fn cycle_backward(&mut self) {
		let current_position = self.list_state.selected().unwrap();

		if 0 == current_position {
			self.list_state.select(Some(self.store.items.len() - 1));
		} else {
			self.list_state.select_previous();
		}
	}
}

fn load_from_store<'a>(store_path: PathBuf) -> Result<PasswordStore<'a>> {
	use walkdir;

	let mut items: HashMap<String, Item<'a>> = HashMap::default();

	for entry in WalkDir::new(store_path) {
		let entry = entry?;

		if entry.clone().into_path().is_dir() {
			continue;
		}

		let file_bytes: Vec<u8> = read(entry.clone().into_path())?;

		// The item before deriving associated items and certain attributes using the
		// filename
		let mut raw_item: OnlineAccount = toml::from_slice(&file_bytes)?;

		// Derive the account, which is practically just the filename
		let identification = entry
			.into_path()
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

/// Entry point: initializes terminal and runs the app safely.
fn main() -> Result<()> {
	color_eyre::install()?;

	enable_raw_mode().context("failed to enable raw mode")?;
	let stdout = io::stdout();

	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

	let res = run_app(&mut terminal);

	res
}

/// Create and run the app with proper error bubbling.
fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
	let mut stdout = io::stdout();

	// Enter the alternative screen for transparent resets
	execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
	terminal.clear()?;

	let mut app = App::new();
	app.run(terminal).context("application run failed")?;

	// Cleanup always restore terminal state before exiting, even on errors
	disable_raw_mode().ok();
	execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;

	Ok(())
}
