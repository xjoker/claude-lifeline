use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "claude-lifeline",
    version,
    about = "Claude Code companion — statusline, TUI dashboard, config, self-update",
    long_about = None,
    disable_help_subcommand = true,
)]
pub struct Cli {
    /// Hidden flag for background update probe (spawned by statusline path)
    #[arg(long, hide = true)]
    pub check_update: bool,

    /// Emit a JSON snapshot instead of the ANSI status line. Only meaningful for the
    /// default (statusline) command. Stable schema, versioned via `schema_version`.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Render the Claude Code status line from stdin JSON (default when no subcommand)
    Statusline,
    /// Launch the interactive TUI dashboard
    Tui,
    /// Manage user configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Check for updates or upgrade the installed binary
    Update {
        #[command(subcommand)]
        action: Option<UpdateAction>,
    },
    /// Diagnose installation & Claude Code integration
    Doctor,
    /// Tail a transcript JSONL and pretty-print new messages as they arrive
    Watch {
        /// session_id to follow (file stem under ~/.claude/projects/.../*.jsonl).
        /// Omit to follow the transcript with the most recent modification.
        session_id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Print resolved configuration (defaults merged with file)
    Show,
    /// Print path to the user config file
    Path,
    /// Open config file in $EDITOR (falls back to vi)
    Edit,
    /// Write a default config.toml if one does not exist
    Init,
}

#[derive(Subcommand, Debug)]
pub enum UpdateAction {
    /// Query GitHub release feed and report whether a newer version exists
    Check,
    /// Download the latest release and replace the running binary
    Run {
        /// Skip the latest-version check and force a re-download
        #[arg(long)]
        force: bool,
    },
}

pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    let json = cli.json;
    match cli.command {
        None | Some(Commands::Statusline) => statusline_run(json).await,
        Some(Commands::Tui) => crate::tui::run().await,
        Some(Commands::Config { action }) => {
            crate::config::cli::run(action.unwrap_or(ConfigAction::Show)).await
        }
        Some(Commands::Update { action }) => {
            crate::update::cli::run(action.unwrap_or(UpdateAction::Check)).await
        }
        Some(Commands::Doctor) => crate::doctor::run().await,
        Some(Commands::Watch { session_id }) => crate::watch::run(session_id).await,
    }
}

/// Original statusline flow. Errors are swallowed (status line must never write to user terminal).
async fn statusline_run(json: bool) -> anyhow::Result<()> {
    if let Err(e) = statusline_run_inner(json).await {
        eprintln!("claude-lifeline statusline: {e}");
    }
    Ok(())
}

async fn statusline_run_inner(json: bool) -> anyhow::Result<()> {
    let stdin = crate::input::read_stdin().await?;

    let cwd = stdin
        .cwd
        .as_deref()
        .or_else(|| {
            stdin
                .workspace
                .as_ref()
                .and_then(|w| w.current_dir.as_deref())
        });

    let (git, usage) = tokio::join!(
        crate::git::get_git_info(cwd),
        crate::usage::get_usage_data(stdin.rate_limits.as_ref()),
    );

    let config = crate::config::read_config();
    let update_hint = crate::update::check_update_hint();
    crate::update::ensure_cache_exists();

    // Burn-rate history: only record when the stdin payload carried fresh rate_limits
    // (priority 1 in get_usage_data). Cache or API fallbacks would poison the EWMA
    // with stale repeats; the advisor flagged this as a non-negotiable defense.
    let (trend_5h, trend_7d) = record_and_compute_trends(&stdin);

    let ctx = crate::render::RenderContext {
        stdin,
        git,
        usage,
        config,
        update_hint,
        trend_5h,
        trend_7d,
    };
    if json {
        crate::render::render_json(&ctx);
    } else {
        crate::render::render(&ctx);
    }
    Ok(())
}

/// Append one sample to the per-session history (only when stdin supplied fresh
/// rate_limits) and return the resulting trend for each window. Cleanup runs
/// probabilistically here so we don't grow `history/` indefinitely.
fn record_and_compute_trends(
    stdin: &crate::input::StdinData,
) -> (Option<crate::history::TrendInfo>, Option<crate::history::TrendInfo>) {
    let Some(rl) = stdin.rate_limits.as_ref() else {
        return (None, None);
    };
    let five = rl.five_hour.as_ref().and_then(|w| w.used_percentage);
    let seven = rl.seven_day.as_ref().and_then(|w| w.used_percentage);
    if five.is_none() && seven.is_none() {
        return (None, None);
    }

    let sample = crate::history::Sample {
        at: chrono::Utc::now(),
        five_hour: five,
        seven_day: seven,
    };
    let samples = crate::history::append(stdin.session_id.as_deref(), sample);
    crate::history::maybe_cleanup();

    let trend_5h = crate::history::compute_trend(&samples, crate::history::Window::FiveHour);
    let trend_7d = crate::history::compute_trend(&samples, crate::history::Window::SevenDay);
    (trend_5h, trend_7d)
}
