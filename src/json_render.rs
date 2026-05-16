//! `--json` output mode for the statusline path.
//!
//! Schema is stable and versioned via `schema_version`. New fields must be additive so
//! existing consumers keep working without code changes; bump `schema_version` only
//! when an existing field's meaning changes or a required field is removed.
//!
//! Today only `schema_version: 1` exists.

use serde_json::{json, Value};

use crate::config::Thresholds;
use crate::history::{TrendDirection, TrendInfo};
use crate::render::RenderContext;
use crate::usage::{PaceDirection, PaceInfo, WindowUsage};

pub fn build(ctx: &RenderContext) -> Value {
    let model_name = crate::input::get_model_name(&ctx.stdin);
    let project = project_value(ctx);
    let git = git_value(&ctx.git);
    let edits = edits_value(&ctx.stdin);
    let context = context_value(ctx);
    let quotas = quotas_value(ctx);
    let subscription = subscription_value(ctx);
    let extra_usage = extra_usage_value(ctx);

    json!({
        "schema_version": 1,
        "lifeline_version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "model": {
            "display_name": model_name,
            "tier": model_tier(&model_name),
        },
        "project": project,
        "git": git,
        "edits": edits,
        "context": context,
        "quotas": quotas,
        "subscription": subscription,
        "extra_usage": extra_usage,
        "update_hint": ctx.update_hint,
    })
}

fn model_tier(display_name: &str) -> &'static str {
    if display_name.contains("Opus") {
        "opus"
    } else if display_name.contains("Sonnet") {
        "sonnet"
    } else if display_name.contains("Haiku") {
        "haiku"
    } else {
        "other"
    }
}

fn project_value(ctx: &RenderContext) -> Value {
    let cwd = ctx
        .stdin
        .cwd
        .as_deref()
        .or_else(|| {
            ctx.stdin
                .workspace
                .as_ref()
                .and_then(|w| w.current_dir.as_deref())
        });
    let name = cwd
        .and_then(|p| std::path::Path::new(p).file_name())
        .and_then(|n| n.to_str());
    json!({
        "cwd": cwd,
        "name": name,
    })
}

fn git_value(git: &crate::git::GitInfo) -> Value {
    json!({
        "branch": git.branch,
        "dirty": git.is_dirty,
        "ahead": git.ahead,
        "behind": git.behind,
    })
}

fn edits_value(stdin: &crate::input::StdinData) -> Value {
    let cost = stdin.cost.as_ref();
    let added = cost.and_then(|c| c.total_lines_added).unwrap_or(0);
    let removed = cost.and_then(|c| c.total_lines_removed).unwrap_or(0);
    json!({
        "added": added,
        "removed": removed,
    })
}

fn context_value(ctx: &RenderContext) -> Value {
    let pct = crate::input::get_context_percent(&ctx.stdin);
    let t = &ctx.config.thresholds;
    let level = if pct >= t.ctx_red_at {
        "red"
    } else if pct >= t.ctx_yellow_at {
        "yellow"
    } else {
        "green"
    };
    json!({
        "used_percent": pct,
        "level": level,
    })
}

fn quotas_value(ctx: &RenderContext) -> Value {
    let t = &ctx.config.thresholds;
    json!({
        "five_hour": quota_window(
            ctx.usage.five_hour.as_ref(),
            crate::usage::WINDOW_5H_SECS,
            t.five_hour_yellow_at,
            t.five_hour_red_at,
            t.pace_tolerance,
            ctx.trend_5h.as_ref(),
        ),
        "seven_day": quota_window(
            ctx.usage.seven_day.as_ref(),
            crate::usage::WINDOW_7D_SECS,
            t.seven_day_yellow_at,
            t.seven_day_red_at,
            t.pace_tolerance,
            ctx.trend_7d.as_ref(),
        ),
        "seven_day_sonnet": quota_window(
            ctx.usage.seven_day_sonnet.as_ref(),
            crate::usage::WINDOW_7D_SECS,
            t.seven_day_yellow_at,
            t.seven_day_red_at,
            t.pace_tolerance,
            ctx.trend_7d.as_ref(),
        ),
        "seven_day_opus": quota_window(
            ctx.usage.seven_day_opus.as_ref(),
            crate::usage::WINDOW_7D_SECS,
            t.seven_day_yellow_at,
            t.seven_day_red_at,
            t.pace_tolerance,
            ctx.trend_7d.as_ref(),
        ),
    })
}

fn quota_window(
    window: Option<&WindowUsage>,
    window_secs: i64,
    yellow_at: f64,
    red_at: f64,
    pace_tolerance: f64,
    trend: Option<&TrendInfo>,
) -> Value {
    let Some(w) = window else {
        return Value::Null;
    };
    let pace = crate::usage::calc_pace(w, window_secs, pace_tolerance);
    let level = quota_level(w.used_percent, pace.as_ref(), yellow_at, red_at);
    json!({
        "used_percent": w.used_percent,
        "resets_at": w.resets_at.map(|t| t.to_rfc3339()),
        "pace": pace_value(pace.as_ref()),
        "trend": trend_value(trend),
        "level": level,
    })
}

fn trend_value(trend: Option<&TrendInfo>) -> Value {
    let Some(t) = trend else {
        return Value::Null;
    };
    let direction = match t.direction {
        TrendDirection::Accelerating => "accelerating",
        TrendDirection::Decelerating => "decelerating",
        TrendDirection::Flat => "flat",
    };
    json!({
        "direction": direction,
        "short_burn_per_sec": t.short_burn_per_sec,
        "long_burn_per_sec": t.long_burn_per_sec,
        "sample_count": t.sample_count,
        "confident": t.is_confident(),
    })
}

fn pace_value(pace: Option<&PaceInfo>) -> Value {
    let Some(p) = pace else {
        return Value::Null;
    };
    let direction = match p.direction {
        PaceDirection::Over => "over",
        PaceDirection::Under => "under",
        PaceDirection::Normal => "normal",
    };
    json!({
        "pace_percent": p.pace_percent,
        "direction": direction,
        "depletion_eta": p.depletion_eta.map(|t| t.to_rfc3339()),
        "recovery_secs": p.recovery_secs,
    })
}

fn quota_level(
    used: f64,
    pace: Option<&PaceInfo>,
    yellow_at: f64,
    red_at: f64,
) -> &'static str {
    let over = pace.is_some_and(|p| p.direction == PaceDirection::Over);
    if used >= red_at {
        "red"
    } else if over || used >= yellow_at {
        "yellow"
    } else {
        "blue"
    }
}

fn subscription_value(ctx: &RenderContext) -> Value {
    if !ctx.config.display.subscription {
        return Value::Null;
    }
    let Some(cred) = crate::auth::read_credentials() else {
        return Value::Null;
    };
    let label = crate::auth::subscription_label(
        cred.subscription_type.as_deref(),
        cred.rate_limit_tier.as_deref(),
    );
    match label {
        Some(l) => json!({
            "label": l,
            "subscription_type": cred.subscription_type,
            "rate_limit_tier": cred.rate_limit_tier,
        }),
        None => Value::Null,
    }
}

fn extra_usage_value(ctx: &RenderContext) -> Value {
    let Some(extra) = ctx.usage.extra_usage.as_ref() else {
        return Value::Null;
    };
    let t = &ctx.config.thresholds;
    json!({
        "monthly_limit": extra.monthly_limit,
        "used_credits": extra.used_credits,
        "utilization": extra.utilization,
        "currency": extra.currency,
        "level": quota_level_for_extra(extra.utilization, t),
    })
}

fn quota_level_for_extra(util: f64, t: &Thresholds) -> &'static str {
    if util >= t.seven_day_red_at {
        "red"
    } else if util >= t.seven_day_yellow_at {
        "yellow"
    } else {
        "blue"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::git::GitInfo;
    use crate::input::StdinData;
    use crate::usage::UsageData;

    #[test]
    fn model_tier_classification() {
        assert_eq!(model_tier("Opus 4.7 1M"), "opus");
        assert_eq!(model_tier("Sonnet 4.6"), "sonnet");
        assert_eq!(model_tier("Haiku 4.5"), "haiku");
        assert_eq!(model_tier("GLM-4.5"), "other");
    }

    #[test]
    fn quota_level_thresholds() {
        // Below yellow + on pace → blue
        assert_eq!(quota_level(50.0, None, 75.0, 90.0), "blue");
        // Below yellow but over pace → yellow (defensive — quota too soon)
        let over = PaceInfo {
            pace_percent: 10.0,
            direction: PaceDirection::Over,
            depletion_eta: None,
            recovery_secs: Some(60),
        };
        assert_eq!(quota_level(50.0, Some(&over), 75.0, 90.0), "yellow");
        // Above yellow regardless of pace → yellow
        assert_eq!(quota_level(80.0, None, 75.0, 90.0), "yellow");
        // Above red regardless → red
        assert_eq!(quota_level(95.0, None, 75.0, 90.0), "red");
    }

    #[test]
    fn schema_v1_top_level_keys_are_stable() {
        // The JSON schema is a compatibility surface — consumers (tmux configs, IDE
        // plugins, scripts) parse `schema_version` and rely on the documented keys.
        // This test pins the set of top-level keys so accidental rename/remove of a
        // field gets caught here rather than out in user scripts.
        let ctx = empty_ctx();
        let v = build(&ctx);
        let obj = v.as_object().expect("root is an object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "context",
                "edits",
                "extra_usage",
                "git",
                "lifeline_version",
                "model",
                "project",
                "quotas",
                "schema_version",
                "subscription",
                "timestamp",
                "update_hint",
            ]
        );
        // schema_version must remain 1 until a breaking change is intentional.
        assert_eq!(obj["schema_version"].as_u64(), Some(1));
    }

    #[test]
    fn schema_v1_quota_keys_are_stable() {
        // When a window has data, its sub-object must always carry the same field set
        // (callers parse trend/level/pace defensively, but null vs missing matters).
        let mut ctx = empty_ctx();
        ctx.usage.five_hour = Some(crate::usage::WindowUsage {
            used_percent: 12.0,
            resets_at: None,
        });
        let v = build(&ctx);
        let fh = v["quotas"]["five_hour"].as_object().expect("five_hour object");
        let mut keys: Vec<&str> = fh.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["level", "pace", "resets_at", "trend", "used_percent"]);
    }

    fn empty_ctx() -> RenderContext {
        RenderContext {
            stdin: StdinData::default(),
            git: GitInfo::default(),
            usage: UsageData {
                five_hour: None,
                seven_day: None,
                seven_day_sonnet: None,
                seven_day_opus: None,
                extra_usage: None,
            },
            config: Config::default(),
            update_hint: None,
            trend_5h: None,
            trend_7d: None,
        }
    }
}
