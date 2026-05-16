use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::AppState;

pub fn draw(frame: &mut Frame, state: &AppState, area: Rect) {
    let mut lines = Vec::new();

    lines.push(label("data dir", crate::data::paths::lifeline_data_root().display().to_string()));
    lines.push(label("config", crate::data::paths::config_path().display().to_string()));
    lines.push(label("projects", crate::data::paths::projects_root().display().to_string()));

    lines.push(Line::from(""));

    let version = env!("CARGO_PKG_VERSION");
    lines.push(label("version", version.to_string()));
    if let Some(v) = &state.update_hint {
        lines.push(label("latest", v.to_string()));
        lines.push(Line::from(Span::styled(
            "→ run `claude-lifeline update run` to upgrade",
            Style::default().fg(Color::Magenta),
        )));
    } else {
        lines.push(label("latest", "(check pending or up-to-date)".into()));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "tip: TUI does not tail Claude Code logs yet — see ~/.claude/projects for raw transcripts.",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" diagnostics "))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn label(k: &str, v: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{k:<10}"), Style::default().fg(Color::DarkGray)),
        Span::raw(v),
    ])
}
