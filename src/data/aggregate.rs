use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

use super::session::SessionSummary;

#[derive(Debug, Default, Clone)]
pub struct UsageRollup {
    pub total_sessions: usize,
    pub total_messages: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub by_model: Vec<ModelBucket>,
    pub by_project: Vec<ProjectBucket>,
}

#[derive(Debug, Clone)]
pub struct ModelBucket {
    pub model: String,
    pub sessions: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ProjectBucket {
    pub project: String,
    pub sessions: usize,
    pub last_active_at: Option<DateTime<Utc>>,
}

/// Restrict aggregation to sessions active within `cutoff` (None = all-time).
pub fn rollup(sessions: &[SessionSummary], since: Option<DateTime<Utc>>) -> UsageRollup {
    let mut rollup = UsageRollup::default();
    let mut model_acc: HashMap<String, ModelBucket> = HashMap::new();
    let mut project_acc: HashMap<String, ProjectBucket> = HashMap::new();

    for s in sessions {
        if let Some(cutoff) = since {
            if s.last_active_at.map(|t| t < cutoff).unwrap_or(true) {
                continue;
            }
        }
        rollup.total_sessions += 1;
        rollup.total_messages += s.message_count;
        rollup.total_input_tokens += s.total_input_tokens;
        rollup.total_output_tokens += s.total_output_tokens;
        rollup.total_cache_read += s.total_cache_read;
        rollup.total_cache_creation += s.total_cache_creation;

        let model_key = s.model.clone().unwrap_or_else(|| "unknown".into());
        let mb = model_acc.entry(model_key.clone()).or_insert_with(|| ModelBucket {
            model: model_key,
            sessions: 0,
            input_tokens: 0,
            output_tokens: 0,
        });
        mb.sessions += 1;
        mb.input_tokens += s.total_input_tokens;
        mb.output_tokens += s.total_output_tokens;

        if let Some(p) = s.project_dir.as_ref() {
            let pb = project_acc.entry(p.clone()).or_insert_with(|| ProjectBucket {
                project: p.clone(),
                sessions: 0,
                last_active_at: None,
            });
            pb.sessions += 1;
            pb.last_active_at = pb
                .last_active_at
                .map(|cur| cur.max(s.last_active_at.unwrap_or(cur)))
                .or(s.last_active_at);
        }
    }

    rollup.by_model = {
        let mut v: Vec<_> = model_acc.into_values().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.input_tokens + b.output_tokens));
        v
    };
    rollup.by_project = {
        let mut v: Vec<_> = project_acc.into_values().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.last_active_at));
        v
    };
    rollup
}

/// Convenience cutoffs for the TUI summary toggles.
pub fn cutoff_today() -> DateTime<Utc> {
    Utc::now() - Duration::days(1)
}

pub fn cutoff_week() -> DateTime<Utc> {
    Utc::now() - Duration::days(7)
}
