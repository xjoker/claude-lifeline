//! Per-session burn-rate history for trend prediction.
//!
//! Each Claude Code window has a stable `session_id`. We append one sample per
//! statusline invocation to `<claude_root>/claude-lifeline/history/<session_id>.jsonl`,
//! capped at HISTORY_MAX samples. Multi-window safety is "for free": each window
//! writes its own file, so the concurrent-spawn anti-pattern that bit us once never
//! shows up here.
//!
//! Trend is computed via two EWMAs (short / long half-life) — cheaper than linear
//! regression, smoother than single-step deltas, and the only state we need to keep
//! across reads is the sample series itself.
//!
//! Defenses:
//! - Δt > 5 min between samples → drop the sample (system sleep/wake would otherwise
//!   poison the EWMA).
//! - session_id sanitized to `[A-Za-z0-9_-]` for filename safety.
//! - Bounded read: we only ever load HISTORY_MAX lines.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Cap on the number of samples retained per session. At a 15s refreshInterval that's
/// ~10 minutes of history; at 60s it's 40 minutes. File stays under ~4 KiB.
const HISTORY_MAX: usize = 40;

/// Drop any sample whose Δt from the previous one exceeds this. Protects against
/// laptop sleep/wake corrupting the EWMA with an unrealistic burst.
const MAX_SAMPLE_GAP_SECS: i64 = 300;

/// Minimum sample count before we consider the trend trustworthy enough to display.
pub const MIN_CONFIDENT_SAMPLES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrendDirection {
    Accelerating,
    Decelerating,
    Flat,
}

#[derive(Debug, Clone)]
pub struct TrendInfo {
    pub direction: TrendDirection,
    pub short_burn_per_sec: f64,
    pub long_burn_per_sec: f64,
    pub sample_count: usize,
}

impl TrendInfo {
    pub fn is_confident(&self) -> bool {
        self.sample_count >= MIN_CONFIDENT_SAMPLES
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    /// Wall-clock UTC of when this sample was observed.
    pub at: DateTime<Utc>,
    /// `used_percentage` for the 5h window, if known.
    pub five_hour: Option<f64>,
    /// `used_percentage` for the 7d window.
    pub seven_day: Option<f64>,
}

/// Identifier of which window's trend we want.
#[derive(Debug, Clone, Copy)]
pub enum Window {
    FiveHour,
    SevenDay,
}

impl Sample {
    fn read(&self, window: Window) -> Option<f64> {
        match window {
            Window::FiveHour => self.five_hour,
            Window::SevenDay => self.seven_day,
        }
    }
}

/// Resolve the per-session history file. Returns `None` when `session_id` is missing
/// or whitelist-sanitizes to the empty string.
pub fn history_path(session_id: Option<&str>) -> Option<PathBuf> {
    let id = session_id?;
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe.is_empty() {
        return None;
    }
    Some(
        crate::data::paths::lifeline_data_root()
            .join("history")
            .join(format!("{safe}.jsonl")),
    )
}

/// Load up to `HISTORY_MAX` most-recent samples from the per-session file. Returns an
/// empty vector for missing files, parse errors, or oversize lines — the caller treats
/// "no history" as "low confidence" naturally.
pub fn load(session_id: Option<&str>) -> Vec<Sample> {
    use std::io::{BufRead, BufReader, Read};
    let Some(path) = history_path(session_id) else {
        return Vec::new();
    };
    let Ok(file) = std::fs::File::open(&path) else {
        return Vec::new();
    };
    // 64 KiB cap — well above HISTORY_MAX × ~150 bytes per JSON record.
    let reader = BufReader::new(file.take(64 * 1024));
    let mut out: Vec<Sample> = Vec::with_capacity(HISTORY_MAX);
    for line in reader.lines().map_while(Result::ok) {
        if line.is_empty() {
            continue;
        }
        if let Ok(sample) = serde_json::from_str::<Sample>(&line) {
            out.push(sample);
        }
    }
    // Keep tail (most recent) within the cap. We trust the file isn't grossly oversized
    // because we rewrite-on-append, but defend anyway.
    if out.len() > HISTORY_MAX {
        out.drain(..out.len() - HISTORY_MAX);
    }
    out
}

/// Append a new sample and rewrite the file with the trailing window. Returns the
/// post-write sample list so callers can compute the trend without a second read.
///
/// Atomic rename: we write to `<path>.tmp` then rename. Same session writing to the
/// same file twice in flight is theoretically possible if refreshInterval is tiny, but
/// statusline runs are short (< 50ms) and refresh ≥ 5s, so the race is closed.
pub fn append(session_id: Option<&str>, new_sample: Sample) -> Vec<Sample> {
    let Some(path) = history_path(session_id) else {
        return Vec::new();
    };
    let mut samples = load(session_id);
    samples.push(new_sample);
    if samples.len() > HISTORY_MAX {
        samples.drain(..samples.len() - HISTORY_MAX);
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let tmp = path.with_extension("jsonl.tmp");
    let body: String = samples
        .iter()
        .filter_map(|s| serde_json::to_string(s).ok())
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!("{body}\n");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }

    samples
}

/// Compute trend from a sample series. Returns None if there are fewer than 2 usable
/// (gap ≤ MAX_SAMPLE_GAP_SECS) sample pairs — meaning we can't even produce a slope.
pub fn compute_trend(samples: &[Sample], window: Window) -> Option<TrendInfo> {
    if samples.len() < 2 {
        return None;
    }

    // Tunables (kept inline for now — promote to config when the next pass tunes UX).
    const ALPHA_SHORT: f64 = 0.3;
    const ALPHA_LONG: f64 = 0.05;
    /// Display threshold: |short - long| must exceed this fraction of |long| (or a
    /// floor) before we call it a trend. Prevents the arrow from flickering at noise.
    const HYSTERESIS_FACTOR: f64 = 0.5;
    const HYSTERESIS_FLOOR: f64 = 0.005; // 0.5%/s — sane absolute floor

    let mut ewma_short: Option<f64> = None;
    let mut ewma_long: Option<f64> = None;
    let mut valid_pairs = 0usize;

    for pair in samples.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let dt = (b.at - a.at).num_seconds();
        if dt <= 0 || dt > MAX_SAMPLE_GAP_SECS {
            continue;
        }
        let (Some(ua), Some(ub)) = (a.read(window), b.read(window)) else {
            continue;
        };
        // Δused / Δt — units: percent per second
        let burn = (ub - ua) / dt as f64;
        // Treat decreases as zero — windows can reset mid-session and we don't want a
        // negative burn ratchet to flip the long EWMA.
        let burn = burn.max(0.0);

        ewma_short = Some(match ewma_short {
            None => burn,
            Some(prev) => ALPHA_SHORT * burn + (1.0 - ALPHA_SHORT) * prev,
        });
        ewma_long = Some(match ewma_long {
            None => burn,
            Some(prev) => ALPHA_LONG * burn + (1.0 - ALPHA_LONG) * prev,
        });
        valid_pairs += 1;
    }

    let (short, long) = match (ewma_short, ewma_long) {
        (Some(s), Some(l)) => (s, l),
        _ => return None,
    };

    let threshold = (long.abs() * HYSTERESIS_FACTOR).max(HYSTERESIS_FLOOR);
    let direction = if (short - long).abs() <= threshold {
        TrendDirection::Flat
    } else if short > long {
        TrendDirection::Accelerating
    } else {
        TrendDirection::Decelerating
    };

    Some(TrendInfo {
        direction,
        short_burn_per_sec: short,
        long_burn_per_sec: long,
        // +1 because valid_pairs counts intervals, sample count is one more than that.
        sample_count: valid_pairs + 1,
    })
}

/// Probabilistic TTL cleanup: ~1 in 50 invocations sweeps the history dir and removes
/// files older than `max_age_secs`. Cheap, no daemon, no scheduling.
pub fn maybe_cleanup() {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Use the existing wall-clock as a cheap source of randomness — we don't need
    // unpredictability, just spread.
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    if now_nanos % 50 != 0 {
        return;
    }
    let dir = crate::data::paths::lifeline_data_root().join("history");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let cutoff = SystemTime::now() - std::time::Duration::from_secs(24 * 3600);
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                if mtime < cutoff {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sample(at_secs: i64, five: Option<f64>, seven: Option<f64>) -> Sample {
        Sample {
            at: DateTime::from_timestamp(at_secs, 0).unwrap(),
            five_hour: five,
            seven_day: seven,
        }
    }

    #[test]
    fn empty_series_yields_no_trend() {
        assert!(compute_trend(&[], Window::FiveHour).is_none());
        assert!(compute_trend(&[sample(0, Some(5.0), None)], Window::FiveHour).is_none());
    }

    #[test]
    fn sample_gap_too_large_is_skipped() {
        // Two samples spaced 10 min apart — bigger than MAX_SAMPLE_GAP_SECS (5 min).
        // Should yield no usable pairs and therefore no trend.
        let samples = vec![sample(0, Some(5.0), None), sample(600, Some(10.0), None)];
        assert!(compute_trend(&samples, Window::FiveHour).is_none());
    }

    #[test]
    fn accelerating_burn_detected() {
        // Burn rises sharply over the last few samples — short EWMA should outrun long
        // EWMA and we should see "accelerating".
        let mut samples = Vec::new();
        // 10 samples at 15s apart, gentle climb (slope 0.01 %/s)
        for i in 0..10 {
            samples.push(sample(i * 15, Some(i as f64 * 0.15), None));
        }
        // 3 more samples at 15s apart, much steeper climb (slope 0.5 %/s)
        for i in 0..3 {
            samples.push(sample(150 + (i + 1) * 15, Some(1.5 + (i + 1) as f64 * 7.5), None));
        }
        let trend = compute_trend(&samples, Window::FiveHour).expect("trend present");
        assert_eq!(trend.direction, TrendDirection::Accelerating);
        assert!(trend.is_confident());
    }

    #[test]
    fn flat_burn_yields_flat_direction() {
        // Constant slope across all samples — short ≈ long, hysteresis swallows.
        let samples: Vec<Sample> = (0..12)
            .map(|i| sample(i * 15, Some(i as f64 * 0.1), None))
            .collect();
        let trend = compute_trend(&samples, Window::FiveHour).expect("trend present");
        assert_eq!(trend.direction, TrendDirection::Flat);
    }

    #[test]
    fn confidence_threshold() {
        // 2 samples = 1 pair = sample_count 2. Should not be confident.
        let samples = vec![
            sample(0, Some(0.0), None),
            sample(15, Some(1.0), None),
        ];
        let trend = compute_trend(&samples, Window::FiveHour).expect("trend present");
        assert!(!trend.is_confident());
        assert_eq!(trend.sample_count, 2);
    }

    #[test]
    fn missing_window_data_skipped() {
        // 7d data present everywhere; 5h missing on half. compute_trend for 5h should
        // still see a usable subset.
        let samples = vec![
            sample(0, None, Some(0.0)),
            sample(15, None, Some(1.0)),
            sample(30, None, Some(2.0)),
            sample(45, None, Some(3.0)),
        ];
        assert!(compute_trend(&samples, Window::FiveHour).is_none());
        let seven = compute_trend(&samples, Window::SevenDay).expect("seven_day trend");
        assert!(matches!(
            seven.direction,
            TrendDirection::Accelerating | TrendDirection::Flat
        ));
    }

    #[test]
    fn negative_burn_clamped_to_zero() {
        // Quota reset mid-session: used drops from 80 to 5. The reset shouldn't cause a
        // negative burn rate. Should not panic, direction should be Flat or
        // Decelerating but never explode.
        let samples = vec![
            sample(0, Some(80.0), None),
            sample(15, Some(5.0), None),
            sample(30, Some(6.0), None),
            sample(45, Some(7.0), None),
        ];
        let trend = compute_trend(&samples, Window::FiveHour).expect("trend present");
        // Just ensure we got a usable answer and the burn rates are non-negative.
        assert!(trend.short_burn_per_sec >= 0.0);
        assert!(trend.long_burn_per_sec >= 0.0);
        let _ = Duration::seconds(1); // import keeper
    }

    #[test]
    fn session_id_sanitization() {
        assert!(history_path(None).is_none());
        assert!(history_path(Some("")).is_none());
        assert!(history_path(Some("!!!@#$")).is_none()); // all stripped
        let p = history_path(Some("abc-123_DEF")).expect("path");
        assert!(p.to_string_lossy().ends_with("abc-123_DEF.jsonl"));
        let p = history_path(Some("../../etc/passwd")).expect("path");
        // Sanitizer drops dots and slashes — only alnum/-/_ survive.
        assert!(p.to_string_lossy().ends_with("etcpasswd.jsonl"));
    }
}
