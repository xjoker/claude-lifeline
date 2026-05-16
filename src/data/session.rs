use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::Path;

/// One Claude Code session reconstructed from a transcript JSONL file.
///
/// Fields are intentionally minimal: only what current callers (statusline / watch /
/// doctor) actually read. Future features that need richer aggregates (per-session
/// token totals, branch history) should extend this struct alongside the consumer.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_dir: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
}

/// Maximum bytes to consume from a single transcript before we bail. Real transcripts
/// run a few MiB; cap at 32 MiB to keep TUI startup bounded even if a transcript grows
/// pathological.
const TRANSCRIPT_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Walk `<claude_root>/projects/*/*.jsonl` and return one summary per file.
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
    // Newest first by started_at — good enough for the doctor count and the
    // statusline's session lookup needs.
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
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
    let mut model: Option<String> = None;
    let mut project_dir: Option<String> = None;

    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<TranscriptEntry>(&line) else {
            continue;
        };

        if started_at.is_none() {
            if let Some(ts) = entry.timestamp.as_deref().and_then(parse_ts) {
                started_at = Some(ts);
            }
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

        // Early exit: once we've found the few fields we need, the rest of the file
        // is just the per-message stream we don't currently summarise.
        if started_at.is_some() && model.is_some() && project_dir.is_some() {
            break;
        }
    }

    Some(SessionSummary {
        session_id,
        project_dir,
        started_at,
        model,
    })
}

#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    #[serde(default)]
    model: Option<String>,
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
