use ratatui::{layout::Rect, prelude::*, widgets::{Block, Borders, Paragraph, Wrap}};
use crate::models::{Item, OnlineAccount, SocialSecurity, AccountStatus, AuthProvider};
use crate::ui::focus::FocusableField;
use crate::ui::theme::WONKY_SET;

pub struct ItemDetailView<'a> {
	pub item:          &'a Item,
	pub focused_field: Option<FocusableField>,
}

impl<'a> ItemDetailView<'a> {
	pub fn render(&mut self, frame: &mut Frame, area: Rect) {
		match self.item {
			Item::OnlineAccount(account) => self.render_online_account(frame, area, account),
			Item::SocialSecurity(ssn) => self.render_social_security(frame, area, ssn),
		}
	}

	pub fn has_security_fields(&self) -> bool {
		match self.item {
			Item::OnlineAccount(online_account) => {
				online_account.security_questions.is_some()
			}
			Item::SocialSecurity(_social_security) => todo!(),
		}
	}

	// Call this to move focus to next field
	pub fn focus_next(&mut self) {
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
	pub fn focus_prev(&mut self) {
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

	pub fn get_available_fields(&self) -> Vec<FocusableField> {
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
