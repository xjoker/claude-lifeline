use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::cursor::Show;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::config::Config;
use crate::usage::UsageData;

/// Top-level pages. Intentionally minimal — the TUI is a configuration / diagnostics
/// surface, not a session dashboard. Full session monitoring belongs in the prompt
/// statusline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Config,
    Logs,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::Config, Tab::Logs];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Config => "Config",
            Tab::Logs => "Logs",
        }
    }
}

pub struct AppState {
    pub tab: Tab,
    pub config: Config,
    pub usage_live: UsageData,
    pub status_message: Option<String>,
    pub config_cursor: usize,
    pub update_hint: Option<String>,
    pub should_quit: bool,
}

#[derive(Default)]
struct TerminalGuard {
    raw_mode: bool,
    alternate_screen: bool,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.raw_mode {
            disable_raw_mode().ok();
        }
        let mut stdout = io::stdout();
        if self.alternate_screen {
            execute!(stdout, LeaveAlternateScreen).ok();
        }
        execute!(stdout, Show).ok();
    }
}

impl AppState {
    pub async fn load() -> Self {
        let config = crate::config::read_config();
        let usage_live = crate::usage::get_usage_data(None).await;
        let update_hint = crate::update::check_update_hint();

        Self {
            tab: Tab::Config,
            config,
            usage_live,
            status_message: None,
            config_cursor: 0,
            update_hint,
            should_quit: false,
        }
    }

    pub async fn refresh(&mut self) {
        self.config = crate::config::read_config();
        self.usage_live = crate::usage::get_usage_data(None).await;
        self.update_hint = crate::update::check_update_hint();
        self.status_message = Some(format!("refreshed @ {}", chrono::Local::now().format("%H:%M:%S")));
    }

    pub fn next_tab(&mut self) {
        let idx = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.tab = Tab::ALL[(idx + 1) % Tab::ALL.len()];
    }

    pub fn prev_tab(&mut self) {
        let idx = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.tab = Tab::ALL[(idx + Tab::ALL.len() - 1) % Tab::ALL.len()];
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mut guard = TerminalGuard { raw_mode: true, alternate_screen: false };
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    guard.alternate_screen = true;
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    main_loop(&mut terminal).await
}

async fn main_loop<B>(terminal: &mut Terminal<B>) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: Send + Sync + 'static,
{
    let mut state = AppState::load().await;

    loop {
        terminal.draw(|frame| crate::tui::ui::draw(frame, &state))?;

        // Block briefly for input; periodic redraw keeps the clock fresh.
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(&mut state, key).await;
                }
            }
        }

        if state.should_quit {
            break;
        }
    }
    Ok(())
}

async fn handle_key(state: &mut AppState, key: KeyEvent) {
    // Global keys
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        state.should_quit = true;
        return;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            state.should_quit = true;
            return;
        }
        KeyCode::Tab => {
            state.next_tab();
            return;
        }
        KeyCode::BackTab => {
            state.prev_tab();
            return;
        }
        KeyCode::Char('r') => {
            state.refresh().await;
            return;
        }
        KeyCode::Char('1') => state.tab = Tab::Config,
        KeyCode::Char('2') => state.tab = Tab::Logs,
        _ => {}
    }

    // Tab-specific keys
    match state.tab {
        Tab::Config => crate::tui::views::config::handle_key(state, key).await,
        Tab::Logs => {}
    }
}
