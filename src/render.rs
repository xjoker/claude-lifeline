use crate::git::GitInfo;
use crate::input::StdinData;
use crate::usage::UsageData;

// ── ANSI 颜色常量 ──

pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD_WHITE: &str = "\x1b[1;37m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const MAGENTA: &str = "\x1b[35m";
pub const CYAN: &str = "\x1b[36m";
pub const RED: &str = "\x1b[31m";
pub const BRIGHT_BLUE: &str = "\x1b[94m";
pub const BRIGHT_MAGENTA: &str = "\x1b[95m";

// ── 渲染上下文 ──

pub struct RenderContext {
    pub stdin: StdinData,
    pub git: GitInfo,
    pub usage: UsageData,
    pub session_duration: Option<std::time::Duration>,
}

// ── 公共函数 ──

/// 渲染两行状态栏，输出到 stdout
///
/// 行1: [Sonnet 4.6 | Max]  claude-lifeline  git:(main*)
/// 行2: ctx ████████░░ 45%  │  5h ████|██░░ 65%(1h 45m ↑)  │  7d ██|░░░░░░ 22%(4d 3h ↓)
pub fn render(ctx: &RenderContext) {
    // ── 行1 ──
    let model_name = crate::input::get_model_name(&ctx.stdin);

    // [Model | Plan] — 青色
    let model_section = if let Some(plan) = &ctx.usage.plan_name {
        format!("{CYAN}[{model_name} | {plan}]{RESET}")
    } else {
        format!("{CYAN}[{model_name}]{RESET}")
    };

    // 项目名 — 黄色
    let project_name = ctx
        .stdin
        .cwd
        .as_deref()
        .or_else(|| {
            ctx.stdin
                .workspace
                .as_ref()
                .and_then(|w| w.current_dir.as_deref())
        })
        .and_then(|p| std::path::Path::new(p).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let project_display = format!("{YELLOW}{project_name}{RESET}");

    // git 部分 — git:() 品红，分支名青色，ahead/behind
    let git_section = if let Some(branch) = &ctx.git.branch {
        let dirty = if ctx.git.is_dirty { "*" } else { "" };
        let mut ab = String::new();
        if ctx.git.ahead > 0 {
            ab.push_str(&format!(" {GREEN}↑{}{RESET}", ctx.git.ahead));
        }
        if ctx.git.behind > 0 {
            ab.push_str(&format!(" {RED}↓{}{RESET}", ctx.git.behind));
        }
        format!(" {MAGENTA}git:({RESET}{CYAN}{branch}{dirty}{RESET}{MAGENTA}){RESET}{ab}")
    } else {
        String::new()
    };

    // 会话时长 — dim 显示
    let session_section = ctx.session_duration.map(|d| {
        let total_secs = d.as_secs();
        let formatted = if total_secs < 60 {
            "0m".to_string()
        } else if total_secs < 3600 {
            format!("{}m", total_secs / 60)
        } else {
            format!("{}h {}m", total_secs / 3600, (total_secs % 3600) / 60)
        };
        format!(" {DIM}{formatted}{RESET}")
    }).unwrap_or_default();

    let line1 = format!("{model_section} {project_display}{git_section}{session_section}");

    // ── 行2 ──
    let mut segments: Vec<String> = Vec::new();

    // Segment 1: Context（>= 85% 时显示 token 明细）
    let ctx_pct = crate::input::get_context_percent(&ctx.stdin);
    let ctx_color = get_context_color(ctx_pct);
    let ctx_bar = render_bar_with_pace(ctx_pct, None, 10, ctx_color);
    let token_detail = if ctx_pct >= 85.0 {
        ctx.stdin.context_window.as_ref()
            .and_then(|cw| cw.current_usage.as_ref())
            .map(|u| {
                let input = u.input_tokens.unwrap_or(0);
                let cache = u.cache_creation_input_tokens.unwrap_or(0)
                    + u.cache_read_input_tokens.unwrap_or(0);
                format!(" {DIM}(in:{} c:{}){RESET}", format_tokens(input), format_tokens(cache))
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    segments.push(format!(
        "{DIM}ctx{RESET} {ctx_bar} {ctx_color}{:.0}%{RESET}{token_detail}",
        ctx_pct
    ));

    // Segment 2: 5h quota
    if let Some(five_hour) = &ctx.usage.five_hour {
        let pace = crate::usage::calc_pace(five_hour, crate::usage::WINDOW_5H_SECS);
        let over = pace.as_ref().is_some_and(|p| p.direction == crate::usage::PaceDirection::Over);
        let color = get_quota_color_with_pace(five_hour.used_percent, over);
        let pace_pct = pace.as_ref().map(|p| p.pace_percent);
        let bar = render_bar_with_pace(five_hour.used_percent, pace_pct, 10, color);
        let suffix = format_quota_suffix(&five_hour.resets_at, &pace);
        let alert = if over { "!" } else { "" };
        let pace_label = format_pace_label(&pace);

        segments.push(format!(
            "{DIM}5h{RESET} {bar} {color}{:.0}%{alert}{RESET}{pace_label}{suffix}",
            five_hour.used_percent
        ));
    }

    // Segment 3: 7d quota
    if let Some(seven_day) = &ctx.usage.seven_day {
        let pace = crate::usage::calc_pace(seven_day, crate::usage::WINDOW_7D_SECS);
        let over = pace.as_ref().is_some_and(|p| p.direction == crate::usage::PaceDirection::Over);
        let color = get_quota_color_with_pace(seven_day.used_percent, over);
        let pace_pct = pace.as_ref().map(|p| p.pace_percent);
        let bar = render_bar_with_pace(seven_day.used_percent, pace_pct, 10, color);
        let suffix = format_quota_suffix(&seven_day.resets_at, &pace);
        let alert = if over { "!" } else { "" };
        let pace_label = format_pace_label(&pace);

        segments.push(format!(
            "{DIM}7d{RESET} {bar} {color}{:.0}%{alert}{RESET}{pace_label}{suffix}",
            seven_day.used_percent
        ));
    }

    let separator = format!("{DIM} │ {RESET}");
    let line2 = segments.join(&separator);

    println!("{DIM}─────────────────────────────────────────{RESET}");
    println!("{line1}");
    println!("{line2}");
}

// ── 私有辅助函数 ──

/// 格式化 quota 后缀：(重置时间 ETA 耗尽预估)
fn format_quota_suffix(
    resets_at: &Option<chrono::DateTime<chrono::Utc>>,
    pace: &Option<crate::usage::PaceInfo>,
) -> String {
    let reset_str = resets_at
        .as_ref()
        .map(crate::usage::format_reset_time)
        .unwrap_or_default();

    // 耗尽时间预估（ETA 前缀标明是预测值，非实际到期时间）
    let depletion_str = pace.as_ref()
        .and_then(|p| p.depletion_eta.as_ref())
        .map(|eta| {
            let local: chrono::DateTime<chrono::Local> = eta.with_timezone(&chrono::Local);
            let today = chrono::Local::now().date_naive();
            let eta_date = local.date_naive();
            let fmt = if eta_date == today {
                local.format("%H:%M").to_string()
            } else {
                local.format("%-m/%-d %H:%M").to_string()
            };
            format!(" {RED}ETA {fmt}{RESET}")
        })
        .unwrap_or_default();

    if reset_str.is_empty() && depletion_str.is_empty() {
        return String::new();
    }

    let mut inner = String::new();
    if !reset_str.is_empty() {
        inner.push_str(&format!("{DIM}{reset_str}{RESET}"));
    }
    if !depletion_str.is_empty() {
        if !inner.is_empty() {
            inner.push(' ');
        }
        inner.push_str(&depletion_str);
    }
    format!("{DIM}({RESET}{inner}{DIM}){RESET}")
}

/// 格式化配速位置标签：/p15.23%
fn format_pace_label(pace: &Option<crate::usage::PaceInfo>) -> String {
    pace.as_ref()
        .map(|p| format!("{DIM}/p{:.2}%{RESET}", p.pace_percent))
        .unwrap_or_default()
}

/// 格式化 token 数量（K/M 缩写）
fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.0}k", count as f64 / 1_000.0)
    } else {
        format!("{count}")
    }
}

/// 渲染带配速标记的进度条（配速线插入而非替换，不吃掉填充块）
fn render_bar_with_pace(used_pct: f64, pace_pct: Option<f64>, width: usize, color: &str) -> String {
    let used_pos = ((used_pct / 100.0) * width as f64).round() as usize;
    let used_pos = used_pos.min(width);

    let pace_pos = pace_pct.map(|p| {
        let pos = ((p / 100.0) * width as f64).round() as usize;
        pos.min(width)
    });

    let mut result = String::new();

    for i in 0..width {
        // 在该位置前插入配速线
        if Some(i) == pace_pos {
            result.push_str(&format!("{BOLD_WHITE}|{RESET}"));
        }
        if i < used_pos {
            result.push_str(&format!("{color}█{RESET}"));
        } else {
            result.push_str(&format!("{DIM}░{RESET}"));
        }
    }
    // 配速线在末尾
    if pace_pos == Some(width) {
        result.push_str(&format!("{BOLD_WHITE}|{RESET}"));
    }

    result
}

/// Context 颜色阈值
fn get_context_color(percent: f64) -> &'static str {
    if percent < 70.0 {
        GREEN
    } else if percent < 85.0 {
        YELLOW
    } else {
        RED
    }
}

/// Quota 颜色阈值（考虑超速状态）
fn get_quota_color_with_pace(percent: f64, over_pace: bool) -> &'static str {
    if percent >= 90.0 {
        RED
    } else if over_pace || percent >= 75.0 {
        YELLOW
    } else {
        BRIGHT_BLUE
    }
}
