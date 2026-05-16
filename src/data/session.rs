use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One Claude Code session reconstructed from a transcript JSONL file.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_dir: Option<String>,
    pub transcript_path: PathBuf,
    pub started_at: Option<DateTime<Utc>>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub message_count: usize,
    pub model: Option<String>,
    pub git_branch: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
}

/// Maximum bytes to consume from a single transcript before we bail. Real transcripts
/// run a few MiB; cap at 32 MiB to keep TUI startup bounded even if a transcript grows
/// pathological.
const TRANSCRIPT_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// "Active" heuristic: a transcript's most recent entry timestamp is within this window
/// from now. Tuned for Claude Code's `statusLine.refreshInterval=15s` default — anything
/// not appended to in 10 minutes is realistically a wrapped or abandoned session.
pub const ACTIVE_THRESHOLD: chrono::Duration = chrono::Duration::minutes(10);

/// Return true when the session looks like it's still being driven (last entry within
/// `ACTIVE_THRESHOLD`). Used by the TUI Sessions view to filter out historical noise.
pub fn is_active(summary: &SessionSummary) -> bool {
    let Some(last) = summary.last_active_at else {
        return false;
    };
    chrono::Utc::now() - last <= ACTIVE_THRESHOLD
}

/// Walk `~/.claude/projects/*/*.jsonl` and return one summary per file.
///
/// Returns an empty vec on missing dir or I/O errors — TUI surface is the right place
/// to tell the user, not log spam.
pub fn scan_all_sessions() -> Vec<SessionSummary> {
    let root = crate::data::paths::projects_root();
    if !root.exists() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let walker = walkdir::WalkDir::new(&root)
        .max_depth(3)
        .into_iter()
        .filter_map(Result::ok);
    for entry in walker {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        if let Some(summary) = summarize_transcript(path) {
            out.push(summary);
        }
    }
    out.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    out
}

/// Parse a single transcript JSONL — first/last timestamps, message count, token usage.
///
/// Tolerant: malformed lines are skipped, unknown fields are ignored, missing fields
/// fall back to None. Stops early if we hit `TRANSCRIPT_MAX_BYTES`.
pub fn summarize_transcript(path: &Path) -> Option<SessionSummary> {
    use std::io::{BufRead, BufReader, Read};

    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file.take(TRANSCRIPT_MAX_BYTES));

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut started_at: Option<DateTime<Utc>> = None;
    let mut last_active_at: Option<DateTime<Utc>> = None;
    let mut message_count = 0usize;
    let mut model: Option<String> = None;
    let mut project_dir: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    let mut total_cache_read = 0u64;
    let mut total_cache_creation = 0u64;

    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<TranscriptEntry>(&line) else {
            continue;
        };

        if let Some(ts) = entry.timestamp.as_deref().and_then(parse_ts) {
            started_at.get_or_insert(ts);
            last_active_at = Some(ts);
        }

        // user / assistant / tool_result style messages
        let is_message = matches!(
            entry.entry_type.as_deref(),
            Some("user") | Some("assistant") | Some("tool_use") | Some("tool_result")
        );
        if is_message {
            message_count += 1;
        }

        if model.is_none() {
            if let Some(m) = entry.message.as_ref().and_then(|m| m.model.as_deref()) {
                model = Some(m.to_string());
            }
        }

        if project_dir.is_none() {
            if let Some(cwd) = entry.cwd.as_deref() {
                project_dir = Some(cwd.to_string());
            }
        }

        if git_branch.is_none() {
            if let Some(b) = entry.git_branch.as_deref() {
                git_branch = Some(b.to_string());
            }
        }

        if let Some(usage) = entry.message.as_ref().and_then(|m| m.usage.as_ref()) {
            total_input_tokens += usage.input_tokens.unwrap_or(0);
            total_output_tokens += usage.output_tokens.unwrap_or(0);
            total_cache_read += usage.cache_read_input_tokens.unwrap_or(0);
            total_cache_creation += usage.cache_creation_input_tokens.unwrap_or(0);
        }
    }

    Some(SessionSummary {
        session_id,
        project_dir,
        transcript_path: path.to_path_buf(),
        started_at,
        last_active_at,
        message_count,
        model,
        git_branch,
        total_input_tokens,
        total_output_tokens,
        total_cache_read,
        total_cache_creation,
    })
}

#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(rename = "type", default)]
    entry_type: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default, rename = "gitBranch")]
    git_branch: Option<String>,
    #[serde(default)]
    message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<TranscriptUsage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
