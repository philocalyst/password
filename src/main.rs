use std::io;
use color_eyre::eyre::{Context, Result};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::prelude::*;
use tokio::sync::mpsc;

mod app;
mod models;
mod p2p;
mod ui;

use crate::app::{App, commands::{SyncCommand, SyncResult}};

/// Entry point: initializes terminal and runs the app safely.
#[tokio::main]
async fn main() -> Result<()> {
	color_eyre::install()?;

	enable_raw_mode().context("failed to enable raw mode")?;
	let stdout = io::stdout();

	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

	// Create channels for P2P sync communication
	let (cmd_tx, mut cmd_rx) = mpsc::channel::<SyncCommand>(10);
	let (result_tx, result_rx) = mpsc::channel::<SyncResult>(10);

	// Spawn the P2P sync background task
	tokio::spawn(async move {
		let mut p2p_instance: Option<p2p::P2PSync> = None;

		while let Some(cmd) = cmd_rx.recv().await {
			match cmd {
				SyncCommand::Share(data) => {
					match p2p::P2PSync::new().await {
						Ok(sync) => {
							match sync.share_data(data).await {
								Ok(ticket) => {
									let _ = result_tx.send(SyncResult::TicketGenerated(ticket)).await;
									p2p_instance = Some(sync);
								}
								Err(e) => {
									let _ = result_tx.send(SyncResult::Error(e.to_string())).await;
								}
							}
						}
						Err(e) => {
							let _ = result_tx.send(SyncResult::Error(e.to_string())).await;
						}
					}
				}
				SyncCommand::Receive(ticket) => {
					match p2p::P2PSync::new().await {
						Ok(sync) => {
							match sync.receive_data(&ticket).await {
								Ok(data) => {
									let _ = result_tx.send(SyncResult::DataReceived(data)).await;
									let _ = sync.shutdown().await;
								}
								Err(e) => {
									let _ = result_tx.send(SyncResult::Error(e.to_string())).await;
								}
							}
						}
						Err(e) => {
							let _ = result_tx.send(SyncResult::Error(e.to_string())).await;
						}
					}
				}
				SyncCommand::Cancel => {
					if let Some(sync) = p2p_instance.take() {
						let _ = sync.shutdown().await;
					}
				}
			}
		}
	});

	run_app(&mut terminal, cmd_tx, result_rx)
}

/// Create and run the app with proper error bubbling.
fn run_app(
	terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
	sync_sx: mpsc::Sender<SyncCommand>,
	sync_rx: mpsc::Receiver<SyncResult>,
) -> Result<()> {
	// Enter the alternative screen for transparent resets
	terminal.clear()?;

	let mut app = App::new(sync_sx, sync_rx);
	app.run(terminal).context("application run failed")?;

	// Cleanup always restore terminal state before exiting, even on errors
	disable_raw_mode().ok();

	Ok(())
}
