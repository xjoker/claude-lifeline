//! `claude-lifeline watch [session_id]` — tail a transcript JSONL and pretty-print
//! user / assistant / tool_use / tool_result messages as they're appended. Polls the
//! file every 250ms; deliberately avoids the `notify` crate to keep the dependency
//! surface small.

use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub async fn run(session_id: Option<String>) -> anyhow::Result<()> {
    let target = resolve_target(session_id.as_deref())?;
    let summary = crate::data::session::summarize_transcript(&target);

    if let Some(s) = &summary {
        let project = s
            .project_dir
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        println!(
            "▶ {} · {} · {} · started {}",
            s.session_id,
            s.model.as_deref().unwrap_or("?"),
            project,
            s.started_at
                .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "—".into()),
        );
    } else {
        println!("▶ watching {}", target.display());
    }
    println!("─ following new entries (Ctrl-C to exit) ─");

    follow(&target).await
}

/// Resolve the JSONL path: explicit session_id (file stem match) or most-recently-
/// modified transcript across all projects.
fn resolve_target(session_id: Option<&str>) -> anyhow::Result<PathBuf> {
    let root = crate::data::paths::projects_root();
    if !root.exists() {
        anyhow::bail!("no transcripts found — ~/.claude/projects does not exist");
    }

    let candidates: Vec<(PathBuf, std::time::SystemTime)> = walkdir::WalkDir::new(&root)
        .max_depth(3)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((e.into_path(), mtime))
        })
        .collect();

    if candidates.is_empty() {
        anyhow::bail!("no .jsonl transcripts under {}", root.display());
    }

    if let Some(id) = session_id {
        candidates
            .into_iter()
            .find(|(p, _)| p.file_stem().and_then(|s| s.to_str()) == Some(id))
            .map(|(p, _)| p)
            .ok_or_else(|| anyhow::anyhow!("no transcript with session_id {id}"))
    } else {
        Ok(candidates
            .into_iter()
            .max_by_key(|(_, m)| *m)
            .map(|(p, _)| p)
            .expect("non-empty candidates"))
    }
}

/// Open the file, seek to the end, then poll for new bytes. Each poll appends the
/// freshly-read tail to a line buffer; complete lines (LF-terminated) get parsed and
/// printed. The buffer survives across polls so a half-written line in the middle of
/// an `fwrite` does not get rendered as two malformed records.
async fn follow(path: &Path) -> anyhow::Result<()> {
    let mut file = tokio::fs::File::open(path).await?;
    let initial_len = file.metadata().await?.len();
    file.seek(SeekFrom::Start(initial_len)).await?;
    let mut offset = initial_len;
    let mut line_buf = String::new();
    let mut chunk = [0u8; 16 * 1024];

    loop {
        // Re-stat to detect rotation/truncation. If the file shrank, the user likely
        // started a new session and the path points elsewhere now — best to bail rather
        // than render garbage.
        let cur_len = match file.metadata().await {
            Ok(m) => m.len(),
            Err(e) => {
                eprintln!("watch: stat failed: {e}");
                return Ok(());
            }
        };
        if cur_len < offset {
            eprintln!("watch: transcript shrank ({offset} → {cur_len}) — exiting");
            return Ok(());
        }

        while offset < cur_len {
            let n = file.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            offset += n as u64;

            // Lossy: tool_result blobs can contain non-UTF-8 bytes (binary file diffs etc.)
            line_buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
            drain_complete_lines(&mut line_buf);
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn drain_complete_lines(buf: &mut String) {
    while let Some(idx) = buf.find('\n') {
        let line: String = buf.drain(..=idx).collect();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }
        render_line(trimmed);
    }
}

#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "type", default)]
    entry_type: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    message: Option<EntryMessage>,
    #[serde(default, rename = "toolUseResult")]
    tool_use_result: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct EntryMessage {
    #[serde(default)]
    content: Option<serde_json::Value>,
    #[serde(default)]
    usage: Option<EntryUsage>,
}

#[derive(Debug, Deserialize)]
struct EntryUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
}

fn render_line(line: &str) {
    let entry: Entry = match serde_json::from_str(line) {
        Ok(e) => e,
        Err(_) => return,
    };

    let stamp = entry
        .timestamp
        .as_deref()
        .and_then(parse_ts)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".into());

    match entry.entry_type.as_deref() {
        Some("user") => {
            let user_text = entry
                .message
                .as_ref()
                .and_then(|m| m.content.as_ref())
                .and_then(extract_text);
            if let Some(msg) = user_text {
                println!("{stamp} \x1b[36m›\x1b[0m {}", truncate(&msg, 280));
            } else if let Some(result) = entry.tool_use_result.as_ref() {
                let summary = summarize_tool_result(result);
                println!("{stamp} \x1b[90m  ◀ {}\x1b[0m", truncate(&summary, 220));
            }
        }
        Some("assistant") => {
            let content = entry.message.as_ref().and_then(|m| m.content.as_ref());
            if let Some(text) = content.and_then(extract_text) {
                println!("{stamp} \x1b[35m‹\x1b[0m {}", truncate(&text, 280));
            }
            if let Some(uses) = content.and_then(extract_tool_uses) {
                for u in &uses {
                    println!("{stamp} \x1b[33m  ⚡ {}\x1b[0m {}", u.0, truncate(&u.1, 200));
                }
            }
            if let Some(usage) = entry.message.as_ref().and_then(|m| m.usage.as_ref()) {
                println!(
                    "{stamp} \x1b[90m  ↳ tokens: in {} · out {}\x1b[0m",
                    usage.input_tokens.unwrap_or(0),
                    usage.output_tokens.unwrap_or(0)
                );
            }
        }
        Some("system") => {
            // Most system lines are noise (hooks, sub-agent boundaries). Print a single
            // dim marker rather than the full payload.
            println!("{stamp} \x1b[90m[system]\x1b[0m");
        }
        _ => {}
    }
}

fn parse_ts(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Pull "text" out of an Anthropic message-content shape. Handles three shapes:
///   1. plain string
///   2. array of `{type: "text", text: "..."}` blocks
///   3. array containing tool_use / tool_result blocks (text fragments returned)
fn extract_text(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(t);
                    }
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

/// Returns Vec<(tool_name, single-line input summary)> for tool_use blocks.
fn extract_tool_uses(content: &serde_json::Value) -> Option<Vec<(String, String)>> {
    let items = content.as_array()?;
    let mut out = Vec::new();
    for item in items {
        if item.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
            continue;
        }
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let input = item.get("input").map(summarize_input).unwrap_or_default();
        out.push((name.to_string(), input));
    }
    if out.is_empty() { None } else { Some(out) }
}

fn summarize_input(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            // Show the first 1-3 fields inline for context.
            let mut pieces = Vec::new();
            for (k, val) in map.iter().take(3) {
                pieces.push(format!("{k}={}", inline_value(val, 80)));
            }
            pieces.join(" ")
        }
        _ => inline_value(v, 200),
    }
}

fn inline_value(v: &serde_json::Value, max: usize) -> String {
    let raw = match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let cleaned = raw.replace('\n', "⏎");
    truncate(&cleaned, max)
}

fn summarize_tool_result(v: &serde_json::Value) -> String {
    // tool result envelopes vary — try a few common shapes.
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(s) = v.get("stdout").and_then(|x| x.as_str()) {
        return s.lines().next().unwrap_or("").to_string();
    }
    if let Some(s) = v.get("content").and_then(|x| x.as_str()) {
        return s.lines().next().unwrap_or("").to_string();
    }
    if let Some(arr) = v.get("content").and_then(|x| x.as_array()) {
        for c in arr {
            if let Some(t) = c.get("text").and_then(|x| x.as_str()) {
                return t.lines().next().unwrap_or("").to_string();
            }
        }
    }
    v.to_string()
}

fn truncate(s: &str, max: usize) -> String {
    let cnt = s.chars().count();
    if cnt <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('…');
    out
}
