use std::{collections::HashMap, fs::read, io::{self}, path::PathBuf, time::Duration};

use celes::Country;
use color_eyre::eyre::{Context, Result};
use crossterm::{event::{self, Event, KeyCode}, terminal::{disable_raw_mode, enable_raw_mode}};
use email_address::EmailAddress;
use human_name::Name;
use jiff::civil::Date;
use phonenumber::PhoneNumber;
use ratatui::{layout::Rect, prelude::*, widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap}};
use serde::{Deserialize, de::DeserializeSeed};
use url::Url;
use walkdir::WalkDir;

use crate::ui::{REGULAR_SET, WONKY_SET};

mod ui;

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
struct App {
	should_quit:          bool,
	store:                PasswordStore,
	focused:              Components,
	list_state:           ListState,
	detail_focused_field: Option<FocusableField>,
}

enum Components {
	List,
	Content,
}

#[derive(Default)]
struct PasswordStore {
	items: HashMap<String, Item>,
}

#[derive(Deserialize, Clone)]
enum Item {
	OnlineAccount(OnlineAccount),
	SocialSecurity(SocialSecurity),
}

#[derive(Deserialize, Clone)]
struct SocialSecurity {
	account_number:   String,
	#[serde(deserialize_with = "deserialize_name")]
	legal_name:       Option<Name>,
	issuance_date:    Option<Date>,
	country_of_issue: Option<Country>,
}

#[derive(Deserialize, Clone)]
enum AuthProvider {
	Google,
	Apple,
	Facebook,
}

#[derive(Deserialize, Clone)]
struct OnlineAccount {
	#[serde(skip)]
	account:            String,
	username:           Option<String>,
	email:              Option<EmailAddress>,
	phone:              Option<PhoneNumber>,
	sign_in_with:       Option<Vec<AuthProvider>>,
	password:           Option<String>,
	status:             Option<AccountStatus>,
	host_website:       Option<Url>,
	login_pages:        Option<Vec<Url>>,
	security_questions: Option<Vec<SecurityQuestion>>,
	date_created:       Option<Date>,
	two_factor_enabled: Option<bool>,
	associated_items:   Option<Vec<String>>,
	notes:              Option<String>,
}

#[derive(Deserialize, Clone)]
enum AccountStatus {
	Active,
	Deactivated,
}

#[derive(Deserialize, Clone)]
struct SecurityQuestion {
	question: String,
	answer:   String,
}

#[derive(Clone)]
struct ItemList<'a>(&'a HashMap<String, Item>);
pub struct ItemDetailView<'a> {
	item:          &'a Item,
	focused_field: Option<FocusableField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusableField {
	// Online Account fields
	Username,
	Email,
	Phone,
	Password,
	Website,
	Status,
	TwoFactor,
	SignInProviders,
	DateCreated,
	SecurityQuestions,
	Notes,
	// Social Security fields
	AccountNumber,
	LegalName,
	Country,
	IssuanceDate,
}

impl<'a> ItemDetailView<'a> {
	fn render(&mut self, frame: &mut Frame, area: Rect) {
		match self.item {
			Item::OnlineAccount(account) => self.render_online_account(frame, area, account),
			Item::SocialSecurity(ssn) => self.render_social_security(frame, area, ssn),
		}
	}

	fn has_security_fields(&self) -> bool {
		match self.item {
			Item::OnlineAccount(online_account) => {
				online_account.security_questions.is_some()
			}
			Item::SocialSecurity(_social_security) => todo!(),
		}
	}

	// Call this to move focus to next field
	fn focus_next(&mut self) {
		let fields = self.get_available_fields();
		if fields.is_empty() {
			return;
		}

		if let Some(current_idx) =
			self.focused_field.and_then(|f| fields.iter().position(|&field| field == f))
		{
			self.focused_field = Some(fields[(current_idx + 1) % fields.len()]);
		} else {
			self.focused_field = Some(fields[0]);
		}
	}

	// Call this to move focus to previous field
	fn focus_prev(&mut self) {
		let fields = self.get_available_fields();
		if fields.is_empty() {
			return;
		}

		if let Some(current_idx) =
			self.focused_field.and_then(|f| fields.iter().position(|&field| field == f))
		{
			self.focused_field = Some(fields[(current_idx + fields.len() - 1) % fields.len()]);
		} else {
			self.focused_field = Some(fields[fields.len() - 1]);
		}
	}

	fn get_available_fields(&self) -> Vec<FocusableField> {
		match self.item {
			Item::OnlineAccount(account) => {
				let mut fields = vec![];
				if account.username.is_some() {
					fields.push(FocusableField::Username);
				}
				if account.email.is_some() {
					fields.push(FocusableField::Email);
				}
				if account.phone.is_some() {
					fields.push(FocusableField::Phone);
				}
				if account.password.is_some() {
					fields.push(FocusableField::Password);
				}
				if account.host_website.is_some() {
					fields.push(FocusableField::Website);
				}
				if account.status.is_some() {
					fields.push(FocusableField::Status);
				}
				if account.two_factor_enabled.is_some() {
					fields.push(FocusableField::TwoFactor);
				}
				if account.sign_in_with.as_ref().is_some_and(|p| !p.is_empty()) {
					fields.push(FocusableField::SignInProviders);
				}
				if account.date_created.is_some() {
					fields.push(FocusableField::DateCreated);
				}
				if account.security_questions.is_some() {
					fields.push(FocusableField::SecurityQuestions);
				}
				if account.notes.is_some() {
					fields.push(FocusableField::Notes);
				}
				fields
			}
			Item::SocialSecurity(ssn) => {
				let mut fields = vec![FocusableField::AccountNumber];
				if ssn.legal_name.is_some() {
					fields.push(FocusableField::LegalName);
				}
				if ssn.country_of_issue.is_some() {
					fields.push(FocusableField::Country);
				}
				if ssn.issuance_date.is_some() {
					fields.push(FocusableField::IssuanceDate);
				}
				fields
			}
		}
	}

	fn is_focused(&self, field: FocusableField) -> bool { self.focused_field == Some(field) }

	fn get_field_style(&self, field: FocusableField, base_color: Color) -> (Style, Style) {
		if self.is_focused(field) {
			(
				Style::default().fg(Color::Black).bg(base_color).add_modifier(Modifier::BOLD),
				Style::default().fg(base_color).add_modifier(Modifier::BOLD),
			)
		} else {
			(
				Style::default().fg(base_color).add_modifier(Modifier::BOLD),
				Style::default().fg(Color::White),
			)
		}
	}

	fn render_online_account(&self, frame: &mut Frame, area: Rect, account: &OnlineAccount) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Min(10), // Main content
			])
			.split(area);

		// Main content - two columns (if there are security configurations)

		let contraints = match self.has_security_fields() {
			true => [Constraint::Percentage(50), Constraint::Percentage(50)],
			false => [Constraint::Percentage(100), Constraint::Percentage(0)],
		};

		let content_chunks =
			Layout::default().direction(Direction::Horizontal).constraints(contraints).split(chunks[0]);

		// Left column - render fields vertically
		self.render_left_column(frame, content_chunks[0], account);

		// Right column - render security fields vertically
		self.render_right_column(frame, content_chunks[1], account);
	}

	fn render_left_column(&self, frame: &mut Frame, area: Rect, account: &OnlineAccount) {
		let mut constraints = vec![];
		let mut field_count = 0;

		if account.username.is_some() {
			constraints.push(Constraint::Length(3));
			field_count += 1;
		}
		if account.email.is_some() {
			constraints.push(Constraint::Length(3));
			field_count += 1;
		}
		if account.phone.is_some() {
			constraints.push(Constraint::Length(3));
			field_count += 1;
		}
		if account.password.is_some() {
			constraints.push(Constraint::Length(3));
			field_count += 1;
		}
		if account.host_website.is_some() {
			constraints.push(Constraint::Length(3));
			field_count += 1;
		}
		if account.notes.is_some() {
			constraints.push(Constraint::Min(5));
		} else if field_count > 0 {
			constraints.push(Constraint::Min(0));
		}

		let chunks =
			Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);

		let mut chunk_idx = 0;

		// Username
		if let Some(username) = &account.username {
			let (_label_style, value_style) = self.get_field_style(FocusableField::Username, Color::Green);
			let border_style = if self.is_focused(FocusableField::Username) {
				Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![Span::styled(username.as_str(), value_style)]))
				.block(
					Block::default()
						.borders(Borders::ALL)
						.title("USERNAME")
						.title_style(Style::new().white())
						.border_style(border_style),
				);
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Email
		if let Some(email) = &account.email {
			let (label_style, value_style) = self.get_field_style(FocusableField::Email, Color::Green);
			let border_style = if self.is_focused(FocusableField::Email) {
				Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("Email: ", label_style),
				Span::styled(email.to_string(), value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Phone
		if let Some(phone) = &account.phone {
			let (label_style, value_style) = self.get_field_style(FocusableField::Phone, Color::Green);
			let border_style = if self.is_focused(FocusableField::Phone) {
				Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("Phone: ", label_style),
				Span::styled(phone.to_string(), value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Password - shows actual password when focused!
		if let Some(password) = &account.password {
			let (_label_style, value_style) = self.get_field_style(FocusableField::Password, Color::Red);
			let border_style = if self.is_focused(FocusableField::Password) {
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let display_text = if self.is_focused(FocusableField::Password) {
				password.as_str()
			} else {
				"••••••••••••••••"
			};

			let widget = Paragraph::new(Line::from(vec![Span::styled(display_text, value_style)]))
				.block(Block::default().borders(Borders::ALL).title("PASSWORD").border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Website
		if let Some(website) = &account.host_website {
			let (_label_style, _value_style) = self.get_field_style(FocusableField::Website, Color::Blue);
			let border_style = if self.is_focused(FocusableField::Website) {
				Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("🌐 ", Style::default().fg(Color::Blue)),
				Span::styled(
					website.to_string(),
					if self.is_focused(FocusableField::Website) {
						Style::default().fg(Color::Black).bg(Color::Blue).add_modifier(Modifier::BOLD)
					} else {
						Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED)
					},
				),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Notes
		if let Some(notes) = &account.notes {
			let border_style = if self.is_focused(FocusableField::Notes) {
				Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let title_style = if self.is_focused(FocusableField::Notes) {
				Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC)
			};

			let widget = Paragraph::new(notes.as_str())
				.block(
					Block::default()
						.title("NOTES")
						.borders(Borders::ALL)
						.border_style(border_style)
						.border_set(WONKY_SET)
						.title_style(title_style),
				)
				.wrap(Wrap { trim: true })
				.style(Style::default().fg(Color::Gray));
			frame.render_widget(widget, chunks[chunk_idx]);
		}
	}

	fn render_right_column(&self, frame: &mut Frame, area: Rect, account: &OnlineAccount) {
		let mut constraints = vec![];

		if account.status.is_some() {
			constraints.push(Constraint::Length(3));
		}
		if account.two_factor_enabled.is_some() {
			constraints.push(Constraint::Length(3));
		}
		if account.sign_in_with.as_ref().is_some_and(|p| !p.is_empty()) {
			let provider_count = account.sign_in_with.as_ref().map_or(0, |p| p.len());
			constraints.push(Constraint::Length(2 + provider_count as u16));
		}
		if account.date_created.is_some() {
			constraints.push(Constraint::Length(3));
		}
		if account.security_questions.is_some() {
			constraints.push(Constraint::Length(3));
		}
		if !constraints.is_empty() {
			constraints.push(Constraint::Min(0));
		}

		let chunks =
			Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);

		let mut chunk_idx = 0;

		// Status
		if let Some(status) = &account.status {
			let (label_style, _) = self.get_field_style(FocusableField::Status, Color::Gray);
			let border_style = if self.is_focused(FocusableField::Status) {
				Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let (status_text, status_color) = match status {
				AccountStatus::Active => ("● ACTIVE", Color::Green),
				AccountStatus::Deactivated => ("○ DEACTIVATED", Color::Red),
			};

			let value_style = if self.is_focused(FocusableField::Status) {
				Style::default().fg(Color::Black).bg(status_color).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(status_color).add_modifier(Modifier::BOLD)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("Status: ", label_style),
				Span::styled(status_text, value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// 2FA
		if let Some(two_fa) = account.two_factor_enabled {
			let (label_style, _) = self.get_field_style(FocusableField::TwoFactor, Color::Magenta);
			let border_style = if self.is_focused(FocusableField::TwoFactor) {
				Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let (icon, text, color) =
				if two_fa { ("✓", "Enabled", Color::Green) } else { ("✗", "Disabled", Color::Red) };

			let value_style = if self.is_focused(FocusableField::TwoFactor) {
				Style::default().fg(Color::Black).bg(color).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(color).add_modifier(Modifier::BOLD)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("2FA: ", label_style),
				Span::styled(format!("{} {}", icon, text), value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Sign in providers
		if let Some(providers) = &account.sign_in_with
			&& !providers.is_empty() {
				let border_style = if self.is_focused(FocusableField::SignInProviders) {
					Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
				} else {
					Style::default().fg(Color::DarkGray)
				};

				let title_style = if self.is_focused(FocusableField::SignInProviders) {
					Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD)
				} else {
					Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
				};

				let mut lines = vec![];
				for provider in providers {
					let (icon, name, color) = match provider {
						AuthProvider::Google => ("G", "Google", Color::Red),
						AuthProvider::Apple => ("", "Apple", Color::White),
						AuthProvider::Facebook => ("f", "Facebook", Color::Blue),
					};
					lines.push(Line::from(vec![
						Span::styled(icon, Style::default().fg(color).add_modifier(Modifier::BOLD)),
						Span::raw(" "),
						Span::styled(name, Style::default().fg(color)),
					]));
				}

				let widget = Paragraph::new(lines).block(
					Block::default()
						.borders(Borders::ALL)
						.border_style(border_style)
						.title(" Sign in with ")
						.title_style(title_style),
				);
				frame.render_widget(widget, chunks[chunk_idx]);
				chunk_idx += 1;
			}

		// Date created
		if let Some(date) = &account.date_created {
			let (label_style, value_style) =
				self.get_field_style(FocusableField::DateCreated, Color::Magenta);
			let border_style = if self.is_focused(FocusableField::DateCreated) {
				Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("Created: ", label_style),
				Span::styled(date.to_string(), value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Security questions
		if let Some(questions) = &account.security_questions {
			let border_style = if self.is_focused(FocusableField::SecurityQuestions) {
				Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let text_style = if self.is_focused(FocusableField::SecurityQuestions) {
				Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::Yellow)
			};

			let widget = Paragraph::new(Line::from(Span::styled(
				format!("Security Questions: {}", questions.len()),
				text_style,
			)))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, chunks[chunk_idx]);
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

		// Content fields
		let mut constraints = vec![Constraint::Length(3)]; // Account number always present
		if ssn.legal_name.is_some() {
			constraints.push(Constraint::Length(3));
		}
		if ssn.country_of_issue.is_some() {
			constraints.push(Constraint::Length(3));
		}
		if ssn.issuance_date.is_some() {
			constraints.push(Constraint::Length(3));
		}
		constraints.push(Constraint::Min(0));

		let field_chunks =
			Layout::default().direction(Direction::Vertical).constraints(constraints).split(chunks[1]);

		let mut chunk_idx = 0;

		// Account Number - reveals full number when focused
		let (label_style, value_style) =
			self.get_field_style(FocusableField::AccountNumber, Color::Red);
		let border_style = if self.is_focused(FocusableField::AccountNumber) {
			Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
		} else {
			Style::default().fg(Color::DarkGray)
		};

		let display_number = if self.is_focused(FocusableField::AccountNumber) {
			ssn.account_number.as_str()
		} else {
			&format!("***-**-{}", &ssn.account_number[ssn.account_number.len().saturating_sub(4)..])
		};

		let widget = Paragraph::new(Line::from(vec![
			Span::styled("SSN: ", label_style),
			Span::styled(display_number, value_style),
		]))
		.block(Block::default().borders(Borders::ALL).border_style(border_style));
		frame.render_widget(widget, field_chunks[chunk_idx]);
		chunk_idx += 1;

		// Legal Name
		if let Some(name) = &ssn.legal_name {
			let (label_style, value_style) = self.get_field_style(FocusableField::LegalName, Color::Cyan);
			let border_style = if self.is_focused(FocusableField::LegalName) {
				Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("Legal Name: ", label_style),
				Span::styled(name.display_full(), value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, field_chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Country
		if let Some(country) = &ssn.country_of_issue {
			let (label_style, value_style) = self.get_field_style(FocusableField::Country, Color::Cyan);
			let border_style = if self.is_focused(FocusableField::Country) {
				Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("Country: ", label_style),
				Span::styled(country.to_string(), value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, field_chunks[chunk_idx]);
			chunk_idx += 1;
		}

		// Issuance Date
		if let Some(date) = &ssn.issuance_date {
			let (label_style, value_style) =
				self.get_field_style(FocusableField::IssuanceDate, Color::Cyan);
			let border_style = if self.is_focused(FocusableField::IssuanceDate) {
				Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};

			let widget = Paragraph::new(Line::from(vec![
				Span::styled("Issued: ", label_style),
				Span::styled(date.to_string(), value_style),
			]))
			.block(Block::default().borders(Borders::ALL).border_style(border_style));
			frame.render_widget(widget, field_chunks[chunk_idx]);
		}
	}
}

impl<'a> From<ItemList<'a>> for List<'a> {
	fn from(items: ItemList<'a>) -> Self {
		let mut sorted_items: Vec<(&String, &Item)> = items.0.iter().collect();
		sorted_items.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));

		let list_items: Vec<ListItem<'a>> = sorted_items
			.iter()
			.map(|(name, _)| {
				let line =
					Line::from(vec![Span::styled(name.to_string(), Style::default().fg(Color::Gray))]);
				ListItem::new(line)
			})
			.collect();

		let block = Block::new().borders(Borders::ALL).border_set(REGULAR_SET).bg(Color::DarkGray);

		List::new(list_items)
			.block(block)
			.highlight_symbol("!")
			.highlight_style(Style::default().add_modifier(Modifier::BOLD))
	}
}

impl<'a> ItemList<'a> {
	/// Get an item by its index in the alphabetically sorted list
	pub fn get_by_index(&self, index: usize) -> Option<(&String, &Item)> {
		let mut sorted_items: Vec<(&String, &Item)> = self.0.iter().collect();
		sorted_items.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
		sorted_items.get(index).copied()
	}
}

impl App {
	fn get_current_item(&self) -> &Item {
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
	fn get_first_field_for_current_item(&self) -> Option<FocusableField> {
		let item = self.get_current_item();
		let detail_view = ItemDetailView { item, focused_field: None };
		let fields = detail_view.get_available_fields();
		fields.first().copied()
	}

	fn focus_next_field(&mut self) {
		let item = self.get_current_item();
		let mut detail_view = ItemDetailView { item, focused_field: self.detail_focused_field };
		detail_view.focus_next();
		self.detail_focused_field = detail_view.focused_field;
	}

	fn focus_prev_field(&mut self) {
		let item = self.get_current_item();
		let mut detail_view = ItemDetailView { item, focused_field: self.detail_focused_field };
		detail_view.focus_prev();
		self.detail_focused_field = detail_view.focused_field;
	}

	/// Create a new instance with default values.
	fn new() -> Self {
		let mut list = ListState::default();
		list.select(Some(0usize));
		let store = load_from_store(PathBuf::from("./store")).unwrap();

		Self {
			should_quit: false,
			focused: Components::List,
			store,
			list_state: list,
			detail_focused_field: None, // Initialize as None
		}
	}

	/// Run the main event loop until `should_quit` becomes true.
	fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
		const TICK_RATE: Duration = Duration::from_millis(1000);

		while !self.should_quit {
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
		let _block = Block::default().title("Ratatui Example").borders(Borders::ALL);

		let item_list = ItemList(&self.store.items);

		let list: List = item_list.clone().into();

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
	}

	fn copy_field(&self) -> Result<()> {
		use arboard;

		let mut clipboard = arboard::Clipboard::new()?;

		clipboard.set_text(self.get_focused_field_value().unwrap())?;

		Ok(())
	}

	fn get_focused_field_value(&self) -> Option<String> {
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
						.join("\n")
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

	fn handle_key(&mut self, key_event: event::KeyEvent) {
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
				_ => {}
			},
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

fn load_from_store<'a>(store_path: PathBuf) -> Result<PasswordStore> {
	

	let mut items: HashMap<String, Item> = HashMap::default();

	for entry in WalkDir::new(store_path) {
		let entry = entry?;

		if entry.clone().into_path().is_dir() {
			continue;
		}

		let file_bytes: Vec<u8> = read(entry.clone().into_path())?;

		// The item before deriving associated items and certain attributes using the
		// filename
		let raw_item: OnlineAccount = toml::from_slice(&file_bytes)?;

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

	

	run_app(&mut terminal)
}

/// Create and run the app with proper error bubbling.
fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
	let _stdout = io::stdout();

	// Enter the alternative screen for transparent resets
	terminal.clear()?;

	let mut app = App::new();
	app.run(terminal).context("application run failed")?;

	// Cleanup always restore terminal state before exiting, even on errors
	disable_raw_mode().ok();

	Ok(())
}
