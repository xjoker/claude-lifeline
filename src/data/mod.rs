//! Cross-cutting data layer shared by statusline (live read) and TUI (aggregate read).
//!
//! Anti-pattern guard: never persist a single "current session" file. Multi-session
//! Claude Code terminals spawn the statusline binary concurrently and overwrite shared
//! state. Session data is either derived on-the-fly from transcripts or partitioned per
//! session_id under ~/.claude/claude-lifeline/sessions/.

pub mod session;
pub mod aggregate;
pub mod paths;
