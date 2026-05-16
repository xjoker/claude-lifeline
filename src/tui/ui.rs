use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use super::app::{AppState, Tab};

pub fn draw(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Min(0),    // body
            Constraint::Length(1), // footer
        ])
        .split(area);

    draw_tabs(frame, state, layout[0]);
    draw_body(frame, state, layout[1]);
    draw_footer(frame, state, layout[2]);
}

fn draw_tabs(frame: &mut Frame, state: &AppState, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| Line::from(format!(" {}·{} ", i + 1, t.title())))
        .collect();
    let selected = Tab::ALL.iter().position(|t| *t == state.tab).unwrap_or(0);
    let tabs = Tabs::new(titles)
        .select(selected)
        .block(Block::default().borders(Borders::ALL).title(" claude-lifeline "))
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn draw_body(frame: &mut Frame, state: &AppState, area: Rect) {
    match state.tab {
        Tab::Sessions => super::views::sessions::draw(frame, state, area),
        Tab::Usage => super::views::usage::draw(frame, state, area),
        Tab::Config => super::views::config::draw(frame, state, area),
        Tab::Logs => super::views::logs::draw(frame, state, area),
    }
}

fn draw_footer(frame: &mut Frame, state: &AppState, area: Rect) {
    let mut spans = vec![
        Span::styled(" tab/shift-tab", Style::default().fg(Color::Yellow)),
        Span::raw(" switch  "),
        Span::styled("r", Style::default().fg(Color::Yellow)),
        Span::raw(" refresh  "),
        Span::styled("q/esc", Style::default().fg(Color::Yellow)),
        Span::raw(" quit"),
    ];
    if let Some(msg) = &state.status_message {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(msg.clone(), Style::default().fg(Color::Green)));
    }
    if let Some(v) = &state.update_hint {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            format!("↑ {v} available"),
            Style::default().fg(Color::Magenta),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
