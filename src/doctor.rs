//! `claude-lifeline doctor` — diagnose installation and Claude Code integration.

use std::path::PathBuf;

pub async fn run() -> anyhow::Result<()> {
    println!("claude-lifeline {} — diagnostic report", env!("CARGO_PKG_VERSION"));
    println!();

    check_binary_on_path();
    check_data_dir();
    check_config();
    check_credentials();
    check_projects_dir();
    check_claude_settings();

    println!();
    println!("Done. Anything red above is worth addressing.");
    Ok(())
}

fn line(ok: bool, label: &str, detail: &str) {
    let mark = if ok { "ok" } else { "!!" };
    println!("  [{mark}] {label:<28} {detail}");
}

fn check_binary_on_path() {
    let exe = std::env::current_exe().ok();
    match &exe {
        Some(p) => line(true, "binary", &format!("running {}", p.display())),
        None => line(false, "binary", "cannot resolve current_exe()"),
    }

    if let Ok(path) = std::env::var("PATH") {
        let name = if cfg!(windows) { "claude-lifeline.exe" } else { "claude-lifeline" };
        let found = std::env::split_paths(&path).any(|dir| dir.join(name).exists());
        line(found, "PATH lookup", if found { "claude-lifeline found on PATH" } else { "binary not found on PATH — install.sh / install.ps1 not run?" });
    }
}

fn check_data_dir() {
    let dir = crate::data::paths::lifeline_data_root();
    let exists = dir.exists();
    line(exists, "data dir", &format!("{}{}", dir.display(), if exists { "" } else { " (missing; will be created on first run)" }));
}

fn check_config() {
    let path = crate::data::paths::config_path();
    if path.exists() {
        line(true, "config.toml", &format!("{}", path.display()));
    } else {
        line(false, "config.toml", &format!("{} (using defaults — run `claude-lifeline config init` to seed)", path.display()));
    }
}

fn check_credentials() {
    let path = crate::data::paths::credentials_path();
    if path.exists() {
        line(true, "credentials", "found ~/.claude/.credentials.json");
    } else {
        #[cfg(target_os = "macos")]
        {
            line(false, "credentials", "no ~/.claude/.credentials.json (Keychain fallback will be tried)");
            return;
        }
        #[cfg(not(target_os = "macos"))]
        line(false, "credentials", "no ~/.claude/.credentials.json — login via Claude Code first");
    }
}

fn check_projects_dir() {
    let dir = crate::data::paths::projects_root();
    if !dir.exists() {
        line(false, "projects/", "no ~/.claude/projects — no transcripts to scan yet");
        return;
    }
    let count = crate::data::session::scan_all_sessions().len();
    line(true, "projects/", &format!("{count} transcript(s) found"));
}

fn check_claude_settings() {
    let candidates: Vec<PathBuf> = [
        crate::data::paths::claude_root().join("settings.json"),
        crate::data::paths::claude_root().join("settings.local.json"),
    ]
    .into_iter()
    .filter(|p| p.exists())
    .collect();

    if candidates.is_empty() {
        line(false, "CC settings.json", "no ~/.claude/settings.json — statusLine integration not configured");
        return;
    }

    let mut integrated = false;
    for path in &candidates {
        if let Ok(text) = std::fs::read_to_string(path) {
            if text.contains("claude-lifeline") {
                integrated = true;
                line(true, "CC statusLine", &format!("claude-lifeline referenced in {}", path.display()));
                break;
            }
        }
    }
    if !integrated {
        line(false, "CC statusLine", "settings.json present but no statusLine.command referencing claude-lifeline");
    }
}
