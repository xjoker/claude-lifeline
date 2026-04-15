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

    // git 部分 — git:() 品红，分支名青色
    let git_section = if let Some(branch) = &ctx.git.branch {
        let dirty = if ctx.git.is_dirty { "*" } else { "" };
        format!(" {MAGENTA}git:({RESET}{CYAN}{branch}{dirty}{RESET}{MAGENTA}){RESET}")
    } else {
        String::new()
    };

    let line1 = format!("{model_section} {project_display}{git_section}");

    // ── 行2 ──
    let mut segments: Vec<String> = Vec::new();

    // Segment 1: Context
    let ctx_pct = crate::input::get_context_percent(&ctx.stdin);
    let ctx_color = get_context_color(ctx_pct);
    let ctx_bar = render_bar_with_pace(ctx_pct, None, 10, ctx_color);
    segments.push(format!(
        "{DIM}ctx{RESET} {ctx_bar} {ctx_color}{:.0}%{RESET}",
        ctx_pct
    ));

    // Segment 2: 5h quota
    if let Some(five_hour) = &ctx.usage.five_hour {
        let pace = crate::usage::calc_pace(five_hour, crate::usage::WINDOW_5H_SECS);
        let over = pace.as_ref().is_some_and(|p| p.direction == crate::usage::PaceDirection::Over);
        // 超速时进度条变黄
        let color = get_quota_color_with_pace(five_hour.used_percent, over);
        let pace_pct = pace.as_ref().map(|p| p.pace_percent);
        let bar = render_bar_with_pace(five_hour.used_percent, pace_pct, 10, color);
        let suffix = format_quota_suffix(&five_hour.resets_at, &pace);
        // 超速时加 !
        let alert = if over { "!" } else { "" };

        segments.push(format!(
            "{DIM}5h{RESET} {bar} {color}{:.0}%{alert}{RESET}{suffix}",
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

        segments.push(format!(
            "{DIM}7d{RESET} {bar} {color}{:.0}%{alert}{RESET}{suffix}",
            seven_day.used_percent
        ));
    }

    let separator = format!("{DIM} │ {RESET}");
    let line2 = segments.join(&separator);

    println!("{line1}");
    println!("{line2}");
}

// ── 私有辅助函数 ──

/// 格式化 quota 后缀：(重置时间 方向箭头)
fn format_quota_suffix(
    resets_at: &Option<chrono::DateTime<chrono::Utc>>,
    pace: &Option<crate::usage::PaceInfo>,
) -> String {
    let reset_str = resets_at
        .as_ref()
        .map(crate::usage::format_reset_time)
        .unwrap_or_default();

    // 重置时间和方向箭头都没有时，不输出后缀
    let direction_str = pace.as_ref().map(|p| match p.direction {
        crate::usage::PaceDirection::Over => format!("{RED}↑{RESET}"),
        crate::usage::PaceDirection::Under => format!("{GREEN}↓{RESET}"),
        crate::usage::PaceDirection::Normal => String::new(),
    }).unwrap_or_default();

    if reset_str.is_empty() && direction_str.is_empty() {
        return String::new();
    }

    let mut inner = String::new();
    if !reset_str.is_empty() {
        inner.push_str(&format!("{DIM}{reset_str}{RESET}"));
    }
    if !direction_str.is_empty() {
        if !inner.is_empty() {
            inner.push(' ');
        }
        inner.push_str(&direction_str);
    }
    format!("{DIM}({RESET}{inner}{DIM}){RESET}")
}

/// 渲染带配速标记的进度条
fn render_bar_with_pace(used_pct: f64, pace_pct: Option<f64>, width: usize, color: &str) -> String {
    let used_pos = ((used_pct / 100.0) * width as f64).round() as usize;
    let used_pos = used_pos.min(width);

    let pace_pos = pace_pct.map(|p| {
        let pos = ((p / 100.0) * width as f64).round() as usize;
        pos.min(width.saturating_sub(1))
    });

    let mut result = String::new();

    for i in 0..width {
        if Some(i) == pace_pos {
            // Pace marker always rendered as bold white |
            result.push_str(&format!("{BOLD_WHITE}|{RESET}"));
        } else if i < used_pos {
            // Filled block in color
            result.push_str(&format!("{color}█{RESET}"));
        } else {
            // Empty block dim
            result.push_str(&format!("{DIM}░{RESET}"));
        }
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
