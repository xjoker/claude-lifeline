use std::path::PathBuf;

/// `$HOME` (or `%USERPROFILE%` on Windows). Falls back to `/tmp` so we never panic.
pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// Resolve the Claude Code root directory.
///
/// Honors `CLAUDE_CONFIG_DIR` (the same env var Claude Code itself recognises) when set
/// to a non-empty value — every derived path (`projects/`, `claude-lifeline/`,
/// `.credentials.json`) follows along automatically. Falls back to `~/.claude`.
pub fn claude_root() -> PathBuf {
    resolve_claude_root(std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref(), &home_dir())
}

/// Pure resolution helper for testability — never reads env vars or the filesystem.
fn resolve_claude_root(env_value: Option<&str>, home: &std::path::Path) -> PathBuf {
    match env_value.map(str::trim) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home.join(".claude"),
    }
}

/// `<claude_root>/projects`
pub fn projects_root() -> PathBuf {
    claude_root().join("projects")
}

/// `<claude_root>/claude-lifeline`
pub fn lifeline_data_root() -> PathBuf {
    claude_root().join("claude-lifeline")
}

/// `<claude_root>/claude-lifeline/config.toml`
pub fn config_path() -> PathBuf {
    lifeline_data_root().join("config.toml")
}

/// `<claude_root>/.credentials.json`
pub fn credentials_path() -> PathBuf {
    claude_root().join(".credentials.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn env_unset_uses_home_default() {
        let home = Path::new("/Users/alice");
        assert_eq!(resolve_claude_root(None, home), PathBuf::from("/Users/alice/.claude"));
    }

    #[test]
    fn env_empty_uses_home_default() {
        let home = Path::new("/Users/alice");
        assert_eq!(resolve_claude_root(Some(""), home), PathBuf::from("/Users/alice/.claude"));
    }

    #[test]
    fn env_whitespace_uses_home_default() {
        // A user who exports CLAUDE_CONFIG_DIR='   ' likely intended "unset"
        let home = Path::new("/Users/alice");
        assert_eq!(resolve_claude_root(Some("   "), home), PathBuf::from("/Users/alice/.claude"));
    }

    #[test]
    fn env_absolute_path_overrides_home() {
        let home = Path::new("/Users/alice");
        assert_eq!(
            resolve_claude_root(Some("/srv/claude-config"), home),
            PathBuf::from("/srv/claude-config")
        );
    }

    #[test]
    fn env_relative_path_overrides_home() {
        // We pass relative paths through verbatim — that's the caller's choice.
        let home = Path::new("/Users/alice");
        assert_eq!(resolve_claude_root(Some(".cc"), home), PathBuf::from(".cc"));
    }

    #[test]
    fn derived_paths_inherit_root_override() {
        // Sanity check: changing the root must change every derived path.
        // We can't easily mutate the env in parallel tests, so verify the topology by
        // composing resolve_claude_root directly.
        let custom = resolve_claude_root(Some("/srv/cc"), Path::new("/Users/alice"));
        assert_eq!(custom.join("projects"), PathBuf::from("/srv/cc/projects"));
        assert_eq!(
            custom.join("claude-lifeline").join("config.toml"),
            PathBuf::from("/srv/cc/claude-lifeline/config.toml")
        );
        assert_eq!(custom.join(".credentials.json"), PathBuf::from("/srv/cc/.credentials.json"));
    }
}
