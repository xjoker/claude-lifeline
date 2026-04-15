use crate::git::GitInfo;
use crate::input::StdinData;
use crate::usage::UsageData;

// ── ANSI 颜色常量 ──

pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD_WHITE: &str = "\x1b[1;37m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
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

    let model_section = if let Some(plan) = &ctx.usage.plan_name {
        format!("[{} | {}]", model_name, plan)
    } else {
        format!("[{}]", model_name)
    };

    // 项目名：cwd 的最后一个路径组件
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

    // git 部分
    let git_section = if let Some(branch) = &ctx.git.branch {
        let dirty = if ctx.git.is_dirty { "*" } else { "" };
        format!("  git:({branch}{dirty})")
    } else {
        String::new()
    };

    let line1 = format!("{model_section}  {project_name}{git_section}");

    // ── 行2 ──
    let mut segments: Vec<String> = Vec::new();

    // Segment 1: Context
    let ctx_pct = crate::input::get_context_percent(&ctx.stdin);
    let ctx_color = get_context_color(ctx_pct);
    let ctx_bar = render_bar_with_pace(ctx_pct, None, 10, ctx_color);
    segments.push(format!(
        "ctx {ctx_bar} {ctx_color}{:.0}%{RESET}",
        ctx_pct
    ));

    // Segment 2: 5h quota
    if let Some(five_hour) = &ctx.usage.five_hour {
        let color = get_quota_color(five_hour.used_percent);
        let pace = crate::usage::calc_pace(five_hour, crate::usage::WINDOW_5H_SECS);
        let pace_pct = pace.as_ref().map(|p| p.pace_percent);
        let bar = render_bar_with_pace(five_hour.used_percent, pace_pct, 10, color);

        let mut suffix = String::new();
        if let Some(resets_at) = &five_hour.resets_at {
            let reset_str = crate::usage::format_reset_time(resets_at);
            suffix.push_str(&format!("({reset_str}"));
            if let Some(ref p) = pace {
                match p.direction {
                    crate::usage::PaceDirection::Over => {
                        suffix.push_str(&format!(" {RED}↑{RESET}"));
                    }
                    crate::usage::PaceDirection::Under => {
                        suffix.push_str(&format!(" {GREEN}↓{RESET}"));
                    }
                    crate::usage::PaceDirection::Normal => {}
                }
            }
            suffix.push(')');
        }

        segments.push(format!(
            "5h {bar} {color}{:.0}%{RESET}{suffix}",
            five_hour.used_percent
        ));
    }

    // Segment 3: 7d quota
    if let Some(seven_day) = &ctx.usage.seven_day {
        let color = get_quota_color(seven_day.used_percent);
        let pace = crate::usage::calc_pace(seven_day, crate::usage::WINDOW_7D_SECS);
        let pace_pct = pace.as_ref().map(|p| p.pace_percent);
        let bar = render_bar_with_pace(seven_day.used_percent, pace_pct, 10, color);

        let mut suffix = String::new();
        if let Some(resets_at) = &seven_day.resets_at {
            let reset_str = crate::usage::format_reset_time(resets_at);
            suffix.push_str(&format!("({reset_str}"));
            if let Some(ref p) = pace {
                match p.direction {
                    crate::usage::PaceDirection::Over => {
                        suffix.push_str(&format!(" {RED}↑{RESET}"));
                    }
                    crate::usage::PaceDirection::Under => {
                        suffix.push_str(&format!(" {GREEN}↓{RESET}"));
                    }
                    crate::usage::PaceDirection::Normal => {}
                }
            }
            suffix.push(')');
        }

        segments.push(format!(
            "7d {bar} {color}{:.0}%{RESET}{suffix}",
            seven_day.used_percent
        ));
    }

    let separator = format!("{DIM} │ {RESET}");
    let line2 = segments.join(&separator);

    println!("{line1}");
    println!("{line2}");
}

// ── 私有辅助函数 ──

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

/// Quota 颜色阈值
fn get_quota_color(percent: f64) -> &'static str {
    if percent < 75.0 {
        BRIGHT_BLUE
    } else if percent < 90.0 {
        BRIGHT_MAGENTA
    } else {
        RED
    }
}
