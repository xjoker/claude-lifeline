use std::path::PathBuf;

/// `$HOME` (or `%USERPROFILE%` on Windows). Falls back to `/tmp` so we never panic.
pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// `~/.claude`
pub fn claude_root() -> PathBuf {
    home_dir().join(".claude")
}

/// `~/.claude/projects`
pub fn projects_root() -> PathBuf {
    claude_root().join("projects")
}

/// `~/.claude/claude-lifeline`
pub fn lifeline_data_root() -> PathBuf {
    claude_root().join("claude-lifeline")
}

/// `~/.claude/claude-lifeline/config.toml`
pub fn config_path() -> PathBuf {
    lifeline_data_root().join("config.toml")
}

/// `~/.claude/.credentials.json`
pub fn credentials_path() -> PathBuf {
    claude_root().join(".credentials.json")
}
