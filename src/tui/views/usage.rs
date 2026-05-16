use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;

use crate::tui::app::AppState;
use crate::data::aggregate::UsageRollup;

pub fn draw(frame: &mut Frame, state: &AppState, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),  // live quotas
            Constraint::Length(8),  // today / week / all-time totals
            Constraint::Min(6),     // top models / projects
        ])
        .split(area);

    draw_live_quotas(frame, state, layout[0]);
    draw_rollup_totals(frame, state, layout[1]);
    draw_breakdown(frame, state, layout[2]);
}

fn draw_live_quotas(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    quota_gauge(
        frame,
        chunks[0],
        " 5h window ",
        state.usage_live.five_hour.as_ref().map(|w| w.used_percent),
        state.config.thresholds.five_hour_yellow_at,
        state.config.thresholds.five_hour_red_at,
    );
    quota_gauge(
        frame,
        chunks[1],
        " 7d window ",
        state.usage_live.seven_day.as_ref().map(|w| w.used_percent),
        state.config.thresholds.seven_day_yellow_at,
        state.config.thresholds.seven_day_red_at,
    );
    quota_gauge(
        frame,
        chunks[2],
        " 7d Opus ",
        state.usage_live.seven_day_opus.as_ref().map(|w| w.used_percent),
        state.config.thresholds.seven_day_yellow_at,
        state.config.thresholds.seven_day_red_at,
    );
}

fn quota_gauge(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    pct: Option<f64>,
    yellow_at: f64,
    red_at: f64,
) {
    let pct = pct.unwrap_or(0.0).clamp(0.0, 100.0);
    let color = if pct >= red_at {
        Color::Red
    } else if pct >= yellow_at {
        Color::Yellow
    } else {
        Color::Green
    };
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(title.to_string()))
        .gauge_style(Style::default().fg(color).bg(Color::Black))
        .percent(pct as u16)
        .label(format!("{pct:.1}%"));
    frame.render_widget(gauge, area);
}

fn draw_rollup_totals(frame: &mut Frame, state: &AppState, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(area);
    rollup_card(frame, cols[0], " today ", &state.rollup_today);
    rollup_card(frame, cols[1], " 7d ", &state.rollup_week);
    rollup_card(frame, cols[2], " all-time ", &state.rollup_all);
}

fn rollup_card(frame: &mut Frame, area: Rect, title: &str, r: &UsageRollup) {
    let lines = vec![
        Line::from(vec![
            Span::styled("sessions  ", Style::default().fg(Color::DarkGray)),
            Span::raw(r.total_sessions.to_string()),
        ]),
        Line::from(vec![
            Span::styled("messages  ", Style::default().fg(Color::DarkGray)),
            Span::raw(r.total_messages.to_string()),
        ]),
        Line::from(vec![
            Span::styled("input     ", Style::default().fg(Color::DarkGray)),
            Span::raw(fmt(r.total_input_tokens)),
        ]),
        Line::from(vec![
            Span::styled("output    ", Style::default().fg(Color::DarkGray)),
            Span::raw(fmt(r.total_output_tokens)),
        ]),
        Line::from(vec![
            Span::styled("cache r/w ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{} / {}", fmt(r.total_cache_read), fmt(r.total_cache_creation))),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title.to_string())),
        area,
    );
}

fn draw_breakdown(frame: &mut Frame, state: &AppState, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let model_lines: Vec<Line> = state
        .rollup_week
        .by_model
        .iter()
        .take(6)
        .map(|m| {
            Line::from(vec![
                Span::styled(
                    format!("{:<20}", truncate(&m.model, 20)),
                    Style::default().fg(Color::Magenta),
                ),
                Span::raw(format!(
                    " {} sess · {} in · {} out",
                    m.sessions,
                    fmt(m.input_tokens),
                    fmt(m.output_tokens)
                )),
            ])
        })
        .collect();
    let model_lines = if model_lines.is_empty() {
        vec![Line::from(Span::styled(
            "no recent activity",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        model_lines
    };

    let proj_lines: Vec<Line> = state
        .rollup_week
        .by_project
        .iter()
        .take(6)
        .map(|p| {
            let name = std::path::Path::new(&p.project)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&p.project);
            let when = p
                .last_active_at
                .map(|t| t.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "—".into());
            Line::from(vec![
                Span::styled(
                    format!("{:<20}", truncate(name, 20)),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(format!(" {} sess · {}", p.sessions, when)),
            ])
        })
        .collect();
    let proj_lines = if proj_lines.is_empty() {
        vec![Line::from(Span::styled(
            "no recent activity",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        proj_lines
    };

    frame.render_widget(
        Paragraph::new(model_lines)
            .block(Block::default().borders(Borders::ALL).title(" by model · 7d ")),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(proj_lines)
            .block(Block::default().borders(Borders::ALL).title(" by project · 7d ")),
        cols[1],
    );
}

fn fmt(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn truncate(s: &str, max: usize) -> String {
    let cnt = s.chars().count();
    if cnt <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(2);
    let mut out: String = s.chars().take(keep).collect();
    out.push_str("..");
    out
}
