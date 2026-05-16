//! Interactive TUI dashboard. Run via `claude-lifeline tui`.

mod app;
mod ui;
mod views;

pub async fn run() -> anyhow::Result<()> {
    app::run().await
}
