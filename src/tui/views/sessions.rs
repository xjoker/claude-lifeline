use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::AppState;

pub fn handle_key(state: &mut AppState, key: KeyEvent) {
    // `a` toggles the active-only filter regardless of whether the visible list is
    // currently empty — without it the user would be stuck on an empty active list and
    // unable to switch back to the historical view.
    if matches!(key.code, KeyCode::Char('a')) {
        state.show_all_sessions = !state.show_all_sessions;
        state.session_cursor = 0;
        state.status_message = Some(format!(
            "sessions: {}",
            if state.show_all_sessions {
                "showing all"
            } else {
                "active only (last 10 min)"
            }
        ));
        return;
    }

    let visible_len = state.visible_sessions().len();
    if visible_len == 0 {
        return;
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            state.session_cursor = (state.session_cursor + 1) % visible_len;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.session_cursor = state
                .session_cursor
                .checked_sub(1)
                .unwrap_or(visible_len - 1);
        }
        KeyCode::Home => state.session_cursor = 0,
        KeyCode::End => state.session_cursor = visible_len - 1,
        _ => {}
    }
}

pub fn draw(frame: &mut Frame, state: &AppState, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_list(frame, state, layout[0]);
    draw_detail(frame, state, layout[1]);
}

fn draw_list(frame: &mut Frame, state: &AppState, area: Rect) {
    let visible = state.visible_sessions();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|s| {
            let model = s.model.as_deref().unwrap_or("?");
            let project = s
                .project_dir
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let when = s
                .last_active_at
                .map(|t| t.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "—".into());
            let is_active = crate::data::session::is_active(s);
            let dot_color = if is_active { Color::Green } else { Color::DarkGray };
            ListItem::new(Line::from(vec![
                Span::styled("● ", Style::default().fg(dot_color)),
                Span::styled(format!("{when:<11}"), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{project:<18}"),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(model.to_string(), Style::default().fg(Color::Magenta)),
            ]))
        })
        .collect();

    let active_count = state.active_session_count();
    let total = state.sessions.len();
    let title = if state.show_all_sessions {
        format!(" sessions · all {total} ({active_count} active) — press 'a' to filter ")
    } else {
        format!(" sessions · active {active_count}/{total} — press 'a' to show all ")
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");

    let mut ls = ListState::default();
    ls.select(if visible.is_empty() { None } else { Some(state.session_cursor) });
    frame.render_stateful_widget(list, area, &mut ls);
}

fn draw_detail(frame: &mut Frame, state: &AppState, area: Rect) {
    let visible = state.visible_sessions();
    let Some(s) = visible.get(state.session_cursor).copied() else {
        let block = Block::default().borders(Borders::ALL).title(" detail ");
        let msg = if state.sessions.is_empty() {
            "no transcripts under ~/.claude/projects/"
        } else if !state.show_all_sessions {
            "no active sessions in the last 10 min — press 'a' to show all"
        } else {
            "select a session"
        };
        frame.render_widget(Paragraph::new(msg).block(block), area);
        return;
    };

    let total_tokens = s.total_input_tokens + s.total_output_tokens;
    let lines = vec![
        Line::from(vec![
            Span::styled("session_id  ", Style::default().fg(Color::DarkGray)),
            Span::raw(s.session_id.clone()),
        ]),
        Line::from(vec![
            Span::styled("model       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                s.model.clone().unwrap_or_else(|| "?".into()),
                Style::default().fg(Color::Magenta),
            ),
        ]),
        Line::from(vec![
            Span::styled("project     ", Style::default().fg(Color::DarkGray)),
            Span::raw(s.project_dir.clone().unwrap_or_else(|| "?".into())),
        ]),
        Line::from(vec![
            Span::styled("branch      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                s.git_branch.clone().unwrap_or_else(|| "—".into()),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("started     ", Style::default().fg(Color::DarkGray)),
            Span::raw(format_ts(s.started_at)),
        ]),
        Line::from(vec![
            Span::styled("last active ", Style::default().fg(Color::DarkGray)),
            Span::raw(format_ts(s.last_active_at)),
        ]),
        Line::from(vec![
            Span::styled("messages    ", Style::default().fg(Color::DarkGray)),
            Span::raw(s.message_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled("tokens i/o  ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!(
                "{} / {}  (total {})",
                fmt_tokens(s.total_input_tokens),
                fmt_tokens(s.total_output_tokens),
                fmt_tokens(total_tokens)
            )),
        ]),
        Line::from(vec![
            Span::styled("cache r/w   ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!(
                "{} / {}",
                fmt_tokens(s.total_cache_read),
                fmt_tokens(s.total_cache_creation)
            )),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("transcript  ", Style::default().fg(Color::DarkGray)),
            Span::raw(s.transcript_path.display().to_string()),
        ]),
    ];

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" detail "))
        .wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

fn format_ts(ts: Option<chrono::DateTime<chrono::Utc>>) -> String {
    match ts {
        Some(t) => t
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "—".into(),
    }
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
