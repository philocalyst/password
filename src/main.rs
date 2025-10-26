use std::{
    io,
    time::{Duration, Instant},
};

use color_eyre::eyre::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListState, Paragraph},
};

/// Application state. Can be expanded later with UI data.
struct App {
    should_quit: bool,
    store: PasswordStore,
    list_state: ListState,
}

#[derive(Default)]
struct PasswordStore {
    items: Vec<Item>,
}

enum Item {
    Simple(Simple),
}

struct Simple {
    username: String,
    password: String,
}

impl App {
    /// Create a new instance with default values.
    fn new() -> Self {
        // Define the default selected item (the first)
        let mut list = ListState::default();
        list.select(Some(0usize));

        Self {
            should_quit: false,
            store: PasswordStore::default(),
            list_state: list,
        }
    }

    /// Run the main event loop until `should_quit` becomes true.
    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        const TICK_RATE: Duration = Duration::from_millis(1000);

        while !self.should_quit {
            terminal
                .draw(|f| self.render(f))
                .context("failed to draw frame")?;

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
        let area = frame.area();

        // A simple frame for our display
        let block = Block::default()
            .title("Ratatui Example")
            .borders(Borders::ALL);

        let list = List::default()
            .items(["test1", "test2"])
            .highlight_symbol(">")
            .highlight_style(Style::new().bold().green())
            .block(block);

        // Get the list state (Not possible for an unselect to occur)
        let selected_item_idx = self.list_state.selected().unwrap();

        // Determine the item to render (Should always associate with item)
        let selected_item = self.store.items.get(selected_item_idx).unwrap();

        // Pass a snapshot of the state at the time to render
        frame.render_stateful_widget(list, area, &mut self.list_state.clone());
    }

    /// Handle key input and update state.
    fn handle_key(&mut self, key_event: event::KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
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
