use crate::config::{Config, Layout};
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

// ── 渲染上下文 ──

pub struct RenderContext {
    pub stdin: StdinData,
    pub git: GitInfo,
    pub usage: UsageData,
    pub session_duration: Option<std::time::Duration>,
    pub config: Config,
    pub update_hint: Option<String>,
}

// ── 公共函数 ──

/// 渲染两行状态栏，输出到 stdout
///
/// 行1: [Sonnet 4.6 | Max]  claude-lifeline  git:(main*)
/// 行2: ctx ████████░░ 45%  │  5h ████|██░░ 65%(1h 45m ↑)  │  7d ██|░░░░░░ 22%(4d 3h ↓)
pub fn render(ctx: &RenderContext) {
    if ctx.config.display.layout == Layout::Mini {
        render_mini(ctx);
        return;
    }

    // ── 行1 ──
    let model_name = crate::input::get_model_name(&ctx.stdin);

    // [Model] — 青色
    let model_section = format!("{CYAN}[{model_name}]{RESET}");

    // cwd 层级 — 黄色，HOME 替换为 ~
    let cwd_str = ctx
        .stdin
        .cwd
        .as_deref()
        .or_else(|| {
            ctx.stdin
                .workspace
                .as_ref()
                .and_then(|w| w.current_dir.as_deref())
        })
        .unwrap_or("unknown");
    let project_display = format!("{YELLOW}{}{RESET}", abbrev_home(cwd_str));

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

    // Segment 1: Context（可配置，>= 85% 时显示 token 明细）
    if ctx.config.display.context {
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
    }

    // Segment 2: 5h quota（可配置）
    if ctx.config.display.five_hour {
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
    }

    // Segment 3: 7d quota（可配置）
    if ctx.config.display.seven_day {
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
    }

    let separator = format!("{DIM} │ {RESET}");
    let single_line = segments.join(&separator);

    // 升级提示：单行附在末尾，多行时独占一行
    let update_inline = ctx.update_hint.as_ref()
        .map(|v| format!("{separator}{YELLOW}↑{v}{RESET}"))
        .unwrap_or_default();
    let update_standalone = ctx.update_hint.as_ref()
        .map(|v| format!("{YELLOW}↑{v}{RESET}"))
        .unwrap_or_default();

    let use_multi = match ctx.config.display.layout {
        Layout::Multi => true,
        Layout::Single | Layout::Mini => false,
        Layout::Auto => {
            // 优先让终端自己换行处理长行 —— 只有在 line2 会超过 2 物理行时才拆分为每段独占一行
            let width = detect_terminal_width();
            let visual_len = visible_width(&format!("{single_line}{update_inline}"));
            visual_len > width * 2
        }
    };

    println!("{line1}");
    if use_multi {
        for seg in &segments {
            println!("{seg}");
        }
        if !update_standalone.is_empty() {
            println!("{update_standalone}");
        }
    } else {
        println!("{single_line}{update_inline}");
    }
}

/// 探测终端列宽，COLUMNS 环境变量 → /dev/tty → 120 兜底
///
/// COLUMNS 优先，方便 Claude Code / 用户通过环境变量显式覆盖
fn detect_terminal_width() -> usize {
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(w) = cols.parse::<usize>() {
            if w > 0 {
                return w;
            }
        }
    }
    if let Some((terminal_size::Width(w), _)) = terminal_size::terminal_size() {
        if w > 0 {
            return w as usize;
        }
    }
    120
}

/// 剥离 ANSI 转义码后按字符数量估算视觉宽度（窄字符按 1 计，CJK 等宽字符按 2 计）
fn visible_width(s: &str) -> usize {
    let mut width = 0usize;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // 跳过 ESC [ ... <final byte in 0x40..=0x7e>
            if let Some('[') = chars.next() {
                for c2 in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c2) {
                        break;
                    }
                }
            }
            continue;
        }
        width += char_width(c);
    }
    width
}

/// 粗略的字符宽度估算：CJK/全角符号算 2 列，其他算 1 列
fn char_width(c: char) -> usize {
    if c.is_control() {
        return 0;
    }
    let cp = c as u32;
    // 覆盖常见 CJK + 全角符号 + emoji BMP 段，够状态栏用
    let wide = matches!(cp,
        0x1100..=0x115F |   // Hangul Jamo
        0x2E80..=0x303E |   // CJK Radicals / Kangxi
        0x3041..=0x33FF |   // Hiragana/Katakana/CJK Compat
        0x3400..=0x4DBF |   // CJK Ext A
        0x4E00..=0x9FFF |   // CJK Unified
        0xA000..=0xA4CF |   // Yi
        0xAC00..=0xD7A3 |   // Hangul Syllables
        0xF900..=0xFAFF |   // CJK Compat Ideographs
        0xFE30..=0xFE4F |   // CJK Compat Forms
        0xFF00..=0xFF60 |   // Fullwidth Forms
        0xFFE0..=0xFFE6 |
        0x1F300..=0x1F64F | // Emoji
        0x1F900..=0x1F9FF |
        0x20000..=0x2FFFD | // CJK Ext B-F
        0x30000..=0x3FFFD
    );
    if wide { 2 } else { 1 }
}

// ── 私有辅助函数 ──

/// 路径首部 HOME 替换为 `~`，跨平台兼容（HOME / USERPROFILE）
fn abbrev_home(path: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if !home.is_empty() && path.starts_with(&home) {
        let rest = &path[home.len()..];
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

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

    // 恢复时间：超速时显示停工多久可追平配速
    let recovery_str = pace.as_ref()
        .and_then(|p| p.recovery_secs)
        .map(|secs| {
            let formatted = if secs < 60 {
                "1m".to_string()
            } else if secs < 3600 {
                format!("{}m", secs / 60)
            } else {
                format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
            };
            format!(" {YELLOW}wait {formatted}{RESET}")
        })
        .unwrap_or_default();

    if reset_str.is_empty() && depletion_str.is_empty() && recovery_str.is_empty() {
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
    if !recovery_str.is_empty() {
        inner.push_str(&recovery_str);
    }
    format!("{DIM}({RESET}{inner}{DIM}){RESET}")
}

/// 格式化配速位置标签：仅超速时显示 /p15.23%
fn format_pace_label(pace: &Option<crate::usage::PaceInfo>) -> String {
    pace.as_ref()
        .filter(|p| p.direction == crate::usage::PaceDirection::Over)
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
///
/// 同色连续字符批量输出，减少 ANSI 转义码开销（约 3x），改善 Windows 宽度截断问题
fn render_bar_with_pace(used_pct: f64, pace_pct: Option<f64>, width: usize, color: &str) -> String {
    let used_pos = ((used_pct / 100.0) * width as f64).round() as usize;
    let used_pos = used_pos.min(width);

    let pace_pos = pace_pct.map(|p| {
        let pos = ((p / 100.0) * width as f64).round() as usize;
        pos.min(width)
    });

    let mut result = String::new();
    let mut run_color: &str = "";
    let mut run_chars = String::new();

    // 将当前缓冲批量写入 result
    macro_rules! flush_run {
        () => {
            if !run_chars.is_empty() {
                result.push_str(run_color);
                result.push_str(&run_chars);
                result.push_str(RESET);
                run_chars.clear();
            }
        };
    }

    for i in 0..width {
        // 配速线：插入当前位置之前
        if Some(i) == pace_pos {
            flush_run!();
            result.push_str(BOLD_WHITE);
            result.push('|');
            result.push_str(RESET);
        }

        let (ch, ch_color): (char, &str) = if i < used_pos {
            ('█', color)
        } else {
            ('░', DIM)
        };

        // 颜色变化时先刷新缓冲
        if ch_color != run_color {
            flush_run!();
            run_color = ch_color;
        }
        run_chars.push(ch);
    }
    flush_run!();

    // 配速线在末尾
    if pace_pos == Some(width) {
        result.push_str(BOLD_WHITE);
        result.push('|');
        result.push_str(RESET);
    }

    result
}

/// Context 颜色阈值
fn get_context_color(percent: f64) -> &'static str {
    if percent < 60.0 {
        GREEN
    } else if percent < 70.0 {
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

// ── Mini 模式：极简色块单行 ──

// 256-color 钉死 RGB —— 不依赖终端主题映射，所有现代终端（Windows Terminal / iTerm2 /
// Alacritty / Kitty / Linux 终端）渲染一致；仅 Win10 老 cmd.exe ConHost 不支持
// 文字统一 #080808（最深灰），所有 bg 选 mid-saturation 浅色，对比度有保证
const FG_DARK: u8 = 232;
// 模型强度渐变：旗舰 → 平衡 → 轻快
const BG_MODEL_OPUS: u8 = 134;    // #af5fd7 紫红，旗舰
const BG_MODEL_SONNET: u8 = 99;   // #8787ff 紫蓝，平衡
const BG_MODEL_HAIKU: u8 = 38;    // #00afd7 青蓝，轻快
const BG_MODEL_OTHER: u8 = 102;   // #878787 灰，其他/未知
const BG_PROJECT: u8 = 73;     // #5fafaf 灰青
const BG_GIT: u8 = 209;        // #ff875f 暖橙
const BG_CTX_SAFE: u8 = 78;    // #5fd787 春绿
const BG_WARN: u8 = 221;       // #ffd75f 金黄
const BG_DANGER: u8 = 167;     // #d75f5f 印度红
const BG_QUOTA_SAFE: u8 = 110; // #87afd7 天蓝

/// 渲染单个色块：` text `（前后各一空格内边距），256-color SGR
fn block(bg: u8, fg: u8, text: &str) -> String {
    format!("\x1b[48;5;{bg}m\x1b[38;5;{fg}m {text} \x1b[0m")
}

/// 截断字符串，按视觉宽度（CJK 算 2）裁到 max 列，超出时用 `…` 收尾
fn truncate_visual(s: &str, max: usize) -> String {
    let total = s.chars().map(char_width).sum::<usize>();
    if total <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    let limit = max.saturating_sub(1); // 留 1 列给 …
    for c in s.chars() {
        let w = char_width(c);
        if used + w > limit {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(".."); // ASCII 省略号，避免 Unicode `…` 在 Windows 老终端缺字形
    out
}

/// 取模型短名：Opus / Sonnet / Haiku，否则取首词
fn short_model(name: &str) -> String {
    for word in ["Opus", "Sonnet", "Haiku"] {
        if name.contains(word) {
            return word.to_string();
        }
    }
    name.split_whitespace().next().unwrap_or(name).to_string()
}

/// 模型强度色：Opus 紫红 / Sonnet 紫蓝 / Haiku 青蓝 / 其他灰
fn model_block_bg(short: &str) -> u8 {
    match short {
        "Opus" => BG_MODEL_OPUS,
        "Sonnet" => BG_MODEL_SONNET,
        "Haiku" => BG_MODEL_HAIKU,
        _ => BG_MODEL_OTHER,
    }
}

/// ctx 色块底色（统一阈值：<60 绿 / <70 黄 / >=70 红）
fn ctx_block_colors(pct: f64) -> (u8, u8) {
    if pct < 60.0 {
        (BG_CTX_SAFE, FG_DARK)
    } else if pct < 70.0 {
        (BG_WARN, FG_DARK)
    } else {
        (BG_DANGER, FG_DARK)
    }
}

/// quota 色块底色（>=90 红 / 超速或>=75 黄 / 否则蓝）
fn quota_block_colors(pct: f64, over: bool) -> (u8, u8) {
    if pct >= 90.0 {
        (BG_DANGER, FG_DARK)
    } else if over || pct >= 75.0 {
        (BG_WARN, FG_DARK)
    } else {
        (BG_QUOTA_SAFE, FG_DARK)
    }
}

/// quota 色块（5h / 7d）：`U/P% L` 或超速时 `U/P%! L ETA HH:MM`
fn quota_block(w: &crate::usage::WindowUsage, window_secs: i64, label: &str) -> String {
    let pace = crate::usage::calc_pace(w, window_secs);
    let pace_pct = pace.as_ref().map(|p| p.pace_percent).unwrap_or(0.0);
    let over = pace.as_ref().is_some_and(|p| p.direction == crate::usage::PaceDirection::Over);

    let (bg, fg) = quota_block_colors(w.used_percent, over);
    let alert = if over { "!" } else { "" };

    let eta_str = if over {
        pace.as_ref()
            .and_then(|p| p.depletion_eta.as_ref())
            .map(|eta| {
                let local: chrono::DateTime<chrono::Local> = eta.with_timezone(&chrono::Local);
                let today = chrono::Local::now().date_naive();
                let fmt = if local.date_naive() == today {
                    local.format("%H:%M").to_string()
                } else {
                    local.format("%-m/%-d %H:%M").to_string()
                };
                format!(" ETA {fmt}")
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    let text = format!(
        "{:.0}/{:.0}%{alert} {label}{eta_str}",
        w.used_percent, pace_pct
    );
    block(bg, fg, &text)
}

/// Mini 模式：所有信息压缩为色块串，按宽度自适应拆行
///
/// 内部分两组：
///   identity = [model, project, git]   — 灰底身份信息
///   metrics  = [ctx, 5h, 7d, update?]  — 配色随状态切换
/// 单行装得下 → 一行；装不下 → identity 一行 / metrics 一行；仍装不下 → 每段一行
fn render_mini(ctx: &RenderContext) {
    let mut identity: Vec<String> = Vec::new();
    let mut metrics: Vec<String> = Vec::new();

    // 模型短名（按强度配色）
    let model = short_model(&crate::input::get_model_name(&ctx.stdin));
    identity.push(block(model_block_bg(&model), FG_DARK, &model));

    // 项目名（截断到 16 列）
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
    identity.push(block(
        BG_PROJECT,
        FG_DARK,
        &truncate_visual(project_name, 16),
    ));

    // git 段：branch[*][↑N][↓M]，branch 截断到 16 列
    if let Some(branch) = &ctx.git.branch {
        let branch_short = truncate_visual(branch, 16);
        let dirty = if ctx.git.is_dirty { "*" } else { "" };
        let mut suffix = String::new();
        if ctx.git.ahead > 0 {
            suffix.push_str(&format!(" ↑{}", ctx.git.ahead));
        }
        if ctx.git.behind > 0 {
            suffix.push_str(&format!(" ↓{}", ctx.git.behind));
        }
        identity.push(block(
            BG_GIT,
            FG_DARK,
            &format!("{branch_short}{dirty}{suffix}"),
        ));
    }

    // ctx
    if ctx.config.display.context {
        let ctx_pct = crate::input::get_context_percent(&ctx.stdin);
        let (bg, fg) = ctx_block_colors(ctx_pct);
        metrics.push(block(bg, fg, &format!("ctx {ctx_pct:.0}%")));
    }

    // 5h
    if ctx.config.display.five_hour {
        if let Some(w) = &ctx.usage.five_hour {
            metrics.push(quota_block(w, crate::usage::WINDOW_5H_SECS, "5h"));
        }
    }

    // 7d
    if ctx.config.display.seven_day {
        if let Some(w) = &ctx.usage.seven_day {
            metrics.push(quota_block(w, crate::usage::WINDOW_7D_SECS, "7d"));
        }
    }

    // 升级提示
    if let Some(v) = &ctx.update_hint {
        metrics.push(block(BG_WARN, FG_DARK, &format!("↑{v}")));
    }

    // 同色块紧贴时不易区分 → 块间统一插入 1 列空格
    let sep = " ";
    let identity_line = identity.join(sep);
    let metrics_line = metrics.join(sep);
    let single_line = if identity_line.is_empty() {
        metrics_line.clone()
    } else if metrics_line.is_empty() {
        identity_line.clone()
    } else {
        format!("{identity_line}{sep}{metrics_line}")
    };

    let width = detect_terminal_width();

    // 优先单行
    if visible_width(&single_line) <= width {
        println!("{single_line}");
        return;
    }

    // 单行装不下：尝试 identity 一行 + metrics 一行
    let id_w = visible_width(&identity_line);
    let met_w = visible_width(&metrics_line);
    if id_w <= width && met_w <= width {
        if !identity_line.is_empty() {
            println!("{identity_line}");
        }
        if !metrics_line.is_empty() {
            println!("{metrics_line}");
        }
        return;
    }

    // 仍装不下：每段独占一行
    for b in identity.iter().chain(metrics.iter()) {
        println!("{b}");
    }
}
