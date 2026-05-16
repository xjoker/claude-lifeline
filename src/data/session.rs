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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".jsonl")
            .tempfile()
            .expect("create tempfile");
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        f.flush().unwrap();
        f
    }

    #[test]
    fn summarize_extracts_first_seen_fields() {
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-05-17T01:00:00Z","cwd":"/home/me/proj","message":{"model":"claude-opus-4-7","content":"hi"}}"#,
            r#"{"type":"assistant","timestamp":"2026-05-17T01:00:01Z","message":{"model":"claude-opus-4-7"}}"#,
        ]);
        let summary = summarize_transcript(f.path()).expect("summary");
        assert_eq!(summary.project_dir.as_deref(), Some("/home/me/proj"));
        assert_eq!(summary.model.as_deref(), Some("claude-opus-4-7"));
        assert!(summary.started_at.is_some());
    }

    #[test]
    fn malformed_lines_are_skipped() {
        // Parser must be tolerant — a corrupted line shouldn't kill the whole summary.
        let f = write_jsonl(&[
            "this is not JSON at all",
            r#"{"type":"user","timestamp":"2026-05-17T01:00:00Z","cwd":"/x","message":{"model":"sonnet"}}"#,
            r#"{"oops":"missing closing brace""#,
        ]);
        let summary = summarize_transcript(f.path()).expect("summary");
        assert_eq!(summary.project_dir.as_deref(), Some("/x"));
        assert_eq!(summary.model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn unknown_fields_ignored() {
        // CC ships transcripts with many fields we don't model. Make sure adding new
        // ones in CC doesn't break our parser.
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-05-17T01:00:00Z","cwd":"/y","sessionId":"abc","uuid":"x","gitBranch":"main","futureField":42,"message":{"model":"haiku","extra":"junk"}}"#,
        ]);
        let summary = summarize_transcript(f.path()).expect("summary");
        assert_eq!(summary.model.as_deref(), Some("haiku"));
    }

    #[test]
    fn empty_or_missing_file_yields_summary_with_no_fields() {
        let f = write_jsonl(&[]);
        let summary = summarize_transcript(f.path()).expect("summary even when empty");
        // session_id is always set from filename; other fields stay None.
        assert!(summary.started_at.is_none());
        assert!(summary.model.is_none());
        assert!(summary.project_dir.is_none());
    }

    #[test]
    fn nonexistent_path_returns_none() {
        let path = std::path::Path::new("/definitely/does/not/exist.jsonl");
        assert!(summarize_transcript(path).is_none());
    }
}
