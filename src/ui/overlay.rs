use ratatui::{layout::Rect, prelude::*, widgets::{Block, Borders, Paragraph, Wrap, Clear}};
use crate::p2p::SyncState;

pub fn render_sync_overlay(sync_state: &SyncState, frame: &mut Frame) {
	if matches!(sync_state, SyncState::Idle) {
		return;
	}

	// Create a centered modal area
	let area = frame.area();
	let modal_width = 70.min(area.width.saturating_sub(4));
	let modal_height = 14.min(area.height.saturating_sub(4));
	let modal_area = Rect {
		x: (area.width.saturating_sub(modal_width)) / 2,
		y: (area.height.saturating_sub(modal_height)) / 2,
		width: modal_width,
		height: modal_height,
	};

	// Clear the modal area
	frame.render_widget(Clear, modal_area);

	let (title, content, border_color) = match sync_state {
		SyncState::Idle => unreachable!(),
		SyncState::Sharing { ticket } => {
			let content = vec![
				Line::from(vec![
					Span::styled("Sharing your passwords...", Style::default().fg(Color::White)),
				]),
				Line::from(""),
				Line::from(Span::styled(
					"Share this ticket with another device:",
					Style::default().fg(Color::Yellow),
				)),
				Line::from(""),
				Line::from(Span::styled(
					if ticket.len() > 60 { &ticket[..60] } else { ticket },
					Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
				)),
				Line::from(Span::styled(
					if ticket.len() > 60 { &ticket[60..ticket.len().min(120)] } else { "" },
					Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
				)),
				Line::from(""),
				Line::from(Span::styled(
					"Waiting for connection... Press [Esc] to cancel",
					Style::default().fg(Color::DarkGray),
				)),
			];
			("P2P Share ", content, Color::Green)
		}
		SyncState::ReceiveInput { input } => {
			let content = vec![
				Line::from(vec![
					Span::styled("Receive passwords from another device", Style::default().fg(Color::White)),
				]),
				Line::from(""),
				Line::from(Span::styled(
					"Enter the ticket from the sharing device:",
					Style::default().fg(Color::Yellow),
				)),
				Line::from(""),
				Line::from(vec![
					Span::styled("> ", Style::default().fg(Color::Green)),
					Span::styled(input, Style::default().fg(Color::Cyan)),
					Span::styled("█", Style::default().fg(Color::White)),
				]),
				Line::from(""),
				Line::from(""),
				Line::from(Span::styled(
					"[Enter] Connect  [Esc] Cancel  [Ctrl+V] Paste",
					Style::default().fg(Color::DarkGray),
				)),
			];
			("P2P Receive ", content, Color::Blue)
		}
		SyncState::Receiving => {
			let content = vec![
				Line::from(""),
				Line::from(vec![
					Span::styled("Connecting and downloading...", Style::default().fg(Color::White)),
				]),
				Line::from(""),
				Line::from(Span::styled(
					"Please wait while we fetch your passwords.",
					Style::default().fg(Color::DarkGray),
				)),
			];
			("Receiving ", content, Color::Yellow)
		}
		SyncState::Completed { message } => {
			let content = vec![
				Line::from(""),
				Line::from(vec![
					Span::styled("Sync Complete!", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
				]),
				Line::from(""),
				Line::from(Span::styled(message, Style::default().fg(Color::White))),
				Line::from(""),
				Line::from(""),
				Line::from(Span::styled(
					"Press [Enter] or [Esc] to close",
					Style::default().fg(Color::DarkGray),
				)),
			];
			("Success ", content, Color::Green)
		}
		SyncState::Error { message } => {
			let content = vec![
				Line::from(""),
				Line::from(vec![
					Span::styled("❌ ", Style::default().fg(Color::Red)),
					Span::styled("Sync Failed", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
				]),
				Line::from(""),
				Line::from(Span::styled(message, Style::default().fg(Color::White))),
				Line::from(""),
				Line::from(""),
				Line::from(Span::styled(
					"Press [Enter] or [Esc] to close",
					Style::default().fg(Color::DarkGray),
				)),
			];
			("Error ", content, Color::Red)
		}
	};

	let modal = Paragraph::new(content)
		.block(
			Block::default()
				.title(title)
				.title_style(Style::default().fg(border_color).add_modifier(Modifier::BOLD))
				.borders(Borders::ALL)
				.border_style(Style::default().fg(border_color))
				.bg(Color::Black),
		)
		.wrap(Wrap { trim: false });

	frame.render_widget(modal, modal_area);
}
