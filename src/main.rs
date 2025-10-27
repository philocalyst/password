use std::{
    io,
    time::{Duration, Instant},
};

use celes::Country;
use color_eyre::eyre::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use email_address::EmailAddress;
use human_name::Name;
use jiff::civil::Date;
use phonenumber::PhoneNumber;
use ratatui::{
    layout::Rect,
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use url::Url;

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
    OnlineAccount(OnlineAccount),
    SocialSecurity(SocialSecurity),
}

struct SocialSecurity {
    account_number: String,
    legal_name: Name,
    issuance_date: Date,
    country_of_issue: Country,
}

enum AuthProvider {
    Google,
    Apple,
    Facebook,
}

struct OnlineAccount {
    account: String,
    username: Option<String>,
    email: Option<EmailAddress>,
    phone: Option<PhoneNumber>,
    sign_in_with: Option<Vec<AuthProvider>>,
    password: Option<String>,
    status: Option<AccountStatus>,
    website: Option<Url>,
    security_questions: Option<Vec<SecurityQuestion>>,
    date_created: Option<Date>,
    two_factor_enabled: Option<bool>,
    associated_items: Vec<&'a Item>,
    notes: Option<String>,
}

enum AccountStatus {
    Active,
    Deactivated,
}

struct SecurityQuestion {
    question: String,
    answer: String,
}

struct ItemList<'a>(&'a [Item]);

impl<'a> From<ItemList<'a>> for List<'a> {
    fn from(items: ItemList<'a>) -> Self {
        let list_items: Vec<ListItem<'a>> = items
            .0
            .iter()
            .map(|item| match item {
                Item::Simple(Simple {
                    account, username, ..
                }) => {
                    let line = Line::from(vec![
                        Span::styled(account, Style::default().fg(Color::Green)),
                        Span::raw(" | "),
                        Span::styled(username, Style::default().fg(Color::Cyan)),
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

impl App {
    /// Create a new instance with default values.
    fn new() -> Self {
        // Define the default selected item (the first)
        let mut list = ListState::default();
        list.select(Some(0usize));

        Self {
            should_quit: false,
            store: PasswordStore {
                items: {
                    vec![
                        Item::Simple(Simple {
                            account: "GitHub".into(),
                            username: "alice".into(),
                            password: "secret".into(),
                        }),
                        Item::Simple(Simple {
                            account: "Reddit".into(),
                            username: "bob".into(),
                            password: "password".into(),
                        }),
                    ]
                },
            },
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
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref());

        // Apply layout to the full terminal area
        let [area1, area2] = layout.split(frame.area())[..] else {
            panic!("Expected 2 layout regions");
        };

        // A simple frame for our display
        let block = Block::default()
            .title("Ratatui Example")
            .borders(Borders::ALL);

        let list: List = ItemList(&self.store.items).into();

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
