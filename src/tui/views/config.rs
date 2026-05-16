use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::config::DisplayKey;
use crate::tui::app::AppState;

pub async fn handle_key(state: &mut AppState, key: KeyEvent) {
    let items = DisplayKey::ALL.len();
    if items == 0 {
        return;
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            state.config_cursor = (state.config_cursor + 1) % items;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.config_cursor = state
                .config_cursor
                .checked_sub(1)
                .unwrap_or(items - 1);
        }
        KeyCode::Char(' ') | KeyCode::Enter => {
            if let Some(key) = DisplayKey::ALL.get(state.config_cursor) {
                let current = key.get(&state.config.display);
                key.set(&mut state.config.display, !current);
                if let Err(e) = crate::config::write_display(&state.config.display) {
                    state.status_message = Some(format!("save failed: {e}"));
                } else {
                    state.status_message = Some(format!("toggled {} → {}", key.label(), !current));
                }
            }
        }
        _ => {}
    }
}

pub fn draw(frame: &mut Frame, state: &AppState, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let items: Vec<ListItem> = DisplayKey::ALL
        .iter()
        .map(|k| {
            let on = k.get(&state.config.display);
            let mark = if on { "[x]" } else { "[ ]" };
            let style = if on {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::raw(k.label()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" display toggles "),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");

    let mut ls = ListState::default();
    ls.select(Some(state.config_cursor));
    frame.render_stateful_widget(list, layout[0], &mut ls);

    let lines = vec![
        Line::from(Span::styled(
            "thresholds (edit via `claude-lifeline config edit`)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(format!(
            "  ctx       yellow {:>4.0}   red {:>4.0}",
            state.config.thresholds.ctx_yellow_at, state.config.thresholds.ctx_red_at
        )),
        Line::from(format!(
            "  5h quota  yellow {:>4.0}   red {:>4.0}",
            state.config.thresholds.five_hour_yellow_at,
            state.config.thresholds.five_hour_red_at
        )),
        Line::from(format!(
            "  7d quota  yellow {:>4.0}   red {:>4.0}",
            state.config.thresholds.seven_day_yellow_at,
            state.config.thresholds.seven_day_red_at
        )),
        Line::from(format!(
            "  pace tolerance       {:>4.0}",
            state.config.thresholds.pace_tolerance
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  space/enter  toggle the highlighted option",
            Style::default().fg(Color::Yellow),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" thresholds "))
            .wrap(Wrap { trim: false }),
        layout[1],
    );
}
