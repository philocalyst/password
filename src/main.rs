use std::{io, time::{Duration, Instant}};

use celes::Country;
use color_eyre::eyre::{Context, Result};
use crossterm::{event::{self, Event, KeyCode}, terminal::{disable_raw_mode, enable_raw_mode}};
use email_address::EmailAddress;
use human_name::Name;
use jiff::civil::Date;
use phonenumber::PhoneNumber;
use ratatui::{layout::Rect, prelude::*, widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap}};
use url::Url;

/// Application state. Can be expanded later with UI data.
struct App<'a> {
	should_quit: bool,
	store:       PasswordStore<'a>,
	list_state:  ListState,
}

#[derive(Default)]
struct PasswordStore<'a> {
	items: Vec<Item<'a>>,
}

enum Item<'a> {
	OnlineAccount(OnlineAccount<'a>),
	SocialSecurity(SocialSecurity),
}

struct SocialSecurity {
	account_number:   String,
	legal_name:       Option<Name>,
	issuance_date:    Option<Date>,
	country_of_issue: Option<Country>,
}

enum AuthProvider {
	Google,
	Apple,
	Facebook,
}

struct OnlineAccount<'a> {
	account:            String,
	username:           Option<String>,
	email:              Option<EmailAddress>,
	phone:              Option<PhoneNumber>,
	sign_in_with:       Option<Vec<AuthProvider>>,
	password:           Option<String>,
	status:             Option<AccountStatus>,
	website:            Option<Url>,
	security_questions: Option<Vec<SecurityQuestion>>,
	date_created:       Option<Date>,
	two_factor_enabled: Option<bool>,
	associated_items:   Vec<&'a Item<'a>>,
	notes:              Option<String>,
}

enum AccountStatus {
	Active,
	Deactivated,
}

struct SecurityQuestion {
	question: String,
	answer:   String,
}

struct ItemList<'a>(&'a [Item<'a>]);
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

		if let Some(website) = &account.website {
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
		let list_items: Vec<ListItem<'a>> = items
			.0
			.iter()
			.map(|item| match item {
				Item::OnlineAccount(OnlineAccount { account, username, .. }) => {
					let line = Line::from(vec![
						Span::styled(account, Style::default().fg(Color::Green)),
						Span::raw(" | "),
						Span::styled(username.clone().unwrap(), Style::default().fg(Color::Cyan)),
					]);
					ListItem::new(line)
				}
				Item::SocialSecurity(social_security) => {
					let line = Line::from(vec![
						Span::styled("hi", Style::default().fg(Color::Green)),
						Span::raw(" | "),
					]);
					ListItem::new(line)
				}
			})
			.collect();

		List::new(list_items)
			.highlight_symbol("> ")
			.highlight_style(Style::default().add_modifier(Modifier::BOLD))
	}
}

impl<'a> App<'a> {
	/// Create a new instance with default values.
	fn new() -> Self {
		// Define the default selected item (the first)
		let mut list = ListState::default();
		list.select(Some(0usize));

		Self {
			should_quit: false,
			store:       PasswordStore {
				items: {
					vec![
						Item::OnlineAccount(OnlineAccount {
							account:            "GitHub".into(),
							username:           Some("alice_codes".into()),
							email:              Some("alice@example.com".parse().unwrap()),
							phone:              Some("+1-555-0123".parse().unwrap()),
							sign_in_with:       Some(vec![AuthProvider::Google, AuthProvider::Apple]),
							password:           Some("correct-horse-battery-staple".into()),
							status:             Some(AccountStatus::Active),
							website:            Some("https://github.com".parse().unwrap()),
							security_questions: Some(vec![
								SecurityQuestion {
									question: "What was your first pet's name?".into(),
									answer:   "Fluffy".into(),
								},
								SecurityQuestion {
									question: "What city were you born in?".into(),
									answer:   "Springfield".into(),
								},
							]),
							date_created:       Some("2020-03-15".parse().unwrap()),
							two_factor_enabled: Some(true),
							associated_items:   vec![],
							notes:              Some(
								"Primary development account. Remember to rotate SSH keys quarterly.".into(),
							),
						}),
						Item::SocialSecurity(SocialSecurity {
							account_number:   "123-45-6789".into(),
							legal_name:       Some(Name::parse("Alice Marie Johnson").unwrap()),
							issuance_date:    Some("1995-06-12".parse().unwrap()),
							country_of_issue: Some("United States".parse().unwrap()),
						}),
					]
				},
			},
			list_state:  list,
		}
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

		let list: List = ItemList(&self.store.items).into();

		// Get the list state (Not possible for an unselect to occur)
		let selected_item_idx = self.list_state.selected().unwrap();

		// Determine the item to render (Should always associate with item)
		let selected_item = self.store.items.get(selected_item_idx).unwrap();

		// Pass a snapshot of the state at the time to render
		frame.render_stateful_widget(list, area1, &mut self.list_state.clone());
		ItemDetailView(selected_item).render(frame, area2);
	}

	/// Handle key input and update state.
	fn handle_key(&mut self, key_event: event::KeyEvent) {
		match key_event.code {
			KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
			KeyCode::Down => self.cycle_forward(),
			KeyCode::Up => self.cycle_backward(),
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
			self.list_state.select_last();
		} else {
			self.list_state.select_previous();
		}
	}
}

/// Entry point: initializes terminal and runs the app safely.
fn main() -> Result<()> {
	color_eyre::install()?;

	enable_raw_mode().context("failed to enable raw mode")?;
	let stdout = io::stdout();
	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

	let res = run_app(&mut terminal);

	// Always restore terminal state before exiting, even on errors
	disable_raw_mode().ok();
	terminal.show_cursor().ok();

	res
}

/// Create and run the app with proper error bubbling.
fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
	// Clear any existing content on the screen
	terminal.clear().context("failed to clear terminal")?;

	let mut app = App::new();
	app.run(terminal).context("application run failed")
}
