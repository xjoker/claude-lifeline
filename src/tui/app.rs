use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::config::Config;
use crate::data::aggregate::{self, UsageRollup};
use crate::data::session::SessionSummary;
use crate::usage::UsageData;

/// Top-level pages the user can tab between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Sessions,
    Usage,
    Config,
    Logs,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Sessions, Tab::Usage, Tab::Config, Tab::Logs];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Sessions => "Sessions",
            Tab::Usage => "Usage",
            Tab::Config => "Config",
            Tab::Logs => "Logs",
        }
    }
}

pub struct AppState {
    pub tab: Tab,
    /// Full, mtime-sorted scan of every transcript on disk. Sessions view filters this
    /// at render time via `show_all_sessions`; Usage rollups always use the full set.
    pub sessions: Vec<SessionSummary>,
    pub rollup_all: UsageRollup,
    pub rollup_week: UsageRollup,
    pub rollup_today: UsageRollup,
    pub config: Config,
    pub usage_live: UsageData,
    pub status_message: Option<String>,
    pub session_cursor: usize,
    pub config_cursor: usize,
    pub update_hint: Option<String>,
    pub should_quit: bool,
    /// false (default): Sessions view shows only currently-active sessions
    /// (last entry within ACTIVE_THRESHOLD). `a` toggles to true to show all.
    pub show_all_sessions: bool,
}

impl AppState {
    /// Subset of sessions to display on the Sessions tab. Honors `show_all_sessions`.
    pub fn visible_sessions(&self) -> Vec<&SessionSummary> {
        if self.show_all_sessions {
            self.sessions.iter().collect()
        } else {
            self.sessions.iter().filter(|s| crate::data::session::is_active(s)).collect()
        }
    }
}

impl AppState {
    pub fn active_session_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| crate::data::session::is_active(s))
            .count()
    }

    pub async fn load() -> Self {
        let sessions = tokio::task::spawn_blocking(crate::data::session::scan_all_sessions)
            .await
            .unwrap_or_default();
        let rollup_all = aggregate::rollup(&sessions, None);
        let rollup_week = aggregate::rollup(&sessions, Some(aggregate::cutoff_week()));
        let rollup_today = aggregate::rollup(&sessions, Some(aggregate::cutoff_today()));
        let config = crate::config::read_config();
        let usage_live = crate::usage::get_usage_data(None).await;
        let update_hint = crate::update::check_update_hint();

        Self {
            tab: Tab::Sessions,
            sessions,
            rollup_all,
            rollup_week,
            rollup_today,
            config,
            usage_live,
            status_message: None,
            session_cursor: 0,
            config_cursor: 0,
            update_hint,
            should_quit: false,
            show_all_sessions: false,
        }
    }

    pub async fn refresh(&mut self) {
        let sessions = tokio::task::spawn_blocking(crate::data::session::scan_all_sessions)
            .await
            .unwrap_or_default();
        self.rollup_all = aggregate::rollup(&sessions, None);
        self.rollup_week = aggregate::rollup(&sessions, Some(aggregate::cutoff_week()));
        self.rollup_today = aggregate::rollup(&sessions, Some(aggregate::cutoff_today()));
        self.sessions = sessions;
        // Clamp cursor against whichever view is currently visible (active filter
        // shrinks the list considerably).
        let visible_len = self.visible_sessions().len();
        if self.session_cursor >= visible_len {
            self.session_cursor = visible_len.saturating_sub(1);
        }
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
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = main_loop(&mut terminal).await;

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    result
}

async fn main_loop<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> anyhow::Result<()> {
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
        KeyCode::Char('1') => state.tab = Tab::Sessions,
        KeyCode::Char('2') => state.tab = Tab::Usage,
        KeyCode::Char('3') => state.tab = Tab::Config,
        KeyCode::Char('4') => state.tab = Tab::Logs,
        _ => {}
    }

    // Tab-specific keys
    match state.tab {
        Tab::Sessions => crate::tui::views::sessions::handle_key(state, key),
        Tab::Config => crate::tui::views::config::handle_key(state, key).await,
        Tab::Usage | Tab::Logs => {}
    }
}
