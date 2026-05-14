use crate::config::{Config, Thresholds};
use crate::git::GitInfo;
use crate::input::StdinData;
use crate::usage::UsageData;

// ── 渲染上下文 ──

pub struct RenderContext {
    pub stdin: StdinData,
    pub git: GitInfo,
    pub usage: UsageData,
    pub config: Config,
    pub update_hint: Option<String>,
}

// ── 公共函数 ──

pub fn render(ctx: &RenderContext) {
    render_mini(ctx);
}

/// 探测终端列宽，优先级：COLUMNS env → stdin/stdout/stderr tty → /dev/tty → 200 兜底
///
/// Claude Code GUI app 的 hook 子进程 stdin/stdout/stderr 全是 pipe，且无 controlling
/// terminal，三种探测都返回 None。默认 200 倾向于发单行让 CC 按真实宽度自然换行；
/// 真窄终端的代价是 mid-block 截断（罕见，且优于在宽屏上误拆行）
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
    if let Some(w) = probe_tty_width() {
        return w;
    }
    200
}

/// Unix 下打开 /dev/tty 直接 ioctl 查宽度，绕过 CC 的 pipe
#[cfg(unix)]
fn probe_tty_width() -> Option<usize> {
    let f = std::fs::File::open("/dev/tty").ok()?;
    let (terminal_size::Width(w), _) = terminal_size::terminal_size_of(&f)?;
    (w > 0).then_some(w as usize)
}

#[cfg(not(unix))]
fn probe_tty_width() -> Option<usize> {
    None
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

/// 格式化代码行数（细粒度：1.3k 而非 1k）
fn format_lines(count: u64) -> String {
    if count >= 10_000 {
        format!("{:.0}k", count as f64 / 1_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        format!("{count}")
    }
}

/// 生成代码改动量片段（仅当 added>0 或 removed>0）。
/// 返回 (added_formatted, removed_formatted)，如 ("+1.3k", "-344")
fn edit_stats_parts(stdin: &crate::input::StdinData) -> Option<(String, String)> {
    let cost = stdin.cost.as_ref()?;
    let added = cost.total_lines_added.unwrap_or(0);
    let removed = cost.total_lines_removed.unwrap_or(0);
    if added == 0 && removed == 0 {
        return None;
    }
    Some((
        format!("+{}", format_lines(added)),
        format!("-{}", format_lines(removed)),
    ))
}

/// mini stats 块：灰底 + `+X` 绿字 / `-Y` 红字，中间空格保持 bg 连续
fn stats_block(added: &str, removed: &str) -> String {
    format!(
        "\x1b[48;5;{bg}m\x1b[38;5;{green}m {added}\x1b[38;5;{red}m {removed} \x1b[0m",
        bg = BG_STATS,
        green = BG_CTX_SAFE,
        red = BG_DANGER,
    )
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
const BG_STATS: u8 = 238;      // #444444 中性暗灰，stats 块底色

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

/// 直接采用 Claude Code 提供的 display_name（如 "Opus 4.7"、"Sonnet 4.6"、"GLM-4.5"），
/// 仅把扩展上下文标识 `(1M ...)` 压缩为 ` 1M` 让 mini 块更紧凑。
/// 未匹配的格式（第三方模型等）原样返回。
fn short_model(display_name: &str) -> String {
    // 匹配 "(1M" 起、" )" 止的整段（含前导空格），替换为 " 1M"
    if let Some(open) = display_name.find("(1M") {
        if let Some(close_rel) = display_name[open..].find(')') {
            let head = display_name[..open].trim_end();
            let tail = &display_name[open + close_rel + 1..];
            return format!("{head} 1M{tail}");
        }
    }
    display_name.to_string()
}

/// 模型强度色：Opus 紫红 / Sonnet 紫蓝 / Haiku 青蓝 / 其他灰
/// 用 contains 而非 == 以兼容带版本号的 display_name（如 "Opus 4.7 1M"）
fn model_block_bg(name: &str) -> u8 {
    if name.contains("Opus") {
        BG_MODEL_OPUS
    } else if name.contains("Sonnet") {
        BG_MODEL_SONNET
    } else if name.contains("Haiku") {
        BG_MODEL_HAIKU
    } else {
        BG_MODEL_OTHER
    }
}

/// ctx 色块底色（阈值来自配置）
fn ctx_block_colors(pct: f64, t: &Thresholds) -> (u8, u8) {
    if pct >= t.ctx_red_at {
        (BG_DANGER, FG_DARK)
    } else if pct >= t.ctx_yellow_at {
        (BG_WARN, FG_DARK)
    } else {
        (BG_CTX_SAFE, FG_DARK)
    }
}

/// quota 色块底色（阈值来自配置；5h / 7d 独立）
fn quota_block_colors(pct: f64, over: bool, yellow_at: f64, red_at: f64) -> (u8, u8) {
    if pct >= red_at {
        (BG_DANGER, FG_DARK)
    } else if over || pct >= yellow_at {
        (BG_WARN, FG_DARK)
    } else {
        (BG_QUOTA_SAFE, FG_DARK)
    }
}

/// quota 色块（5h / 7d）：极简格式
///   正常: `U/P%`
///   超速: `U/P%! →HH:MM ↓45m`   (ETA 用 → 前缀、wait 用 ↓ 前缀；5h/7d label 砍掉)
/// 两块靠位置/顺序区分：5h 永远在 7d 左边
///
/// pace 由调用方传入，避免在外层判断 over 后内部再算一次（也防止 Utc::now() 跨秒导致两次结果不一致）。
fn quota_block(
    w: &crate::usage::WindowUsage,
    pace: Option<&crate::usage::PaceInfo>,
    label: &str,
    yellow_at: f64,
    red_at: f64,
) -> String {
    let pace_pct = pace.map(|p| p.pace_percent).unwrap_or(0.0);
    let over = pace.is_some_and(|p| p.direction == crate::usage::PaceDirection::Over);

    let (bg, fg) = quota_block_colors(w.used_percent, over, yellow_at, red_at);
    let alert = if over { "!" } else { "" };
    let prefix = if label.is_empty() { String::new() } else { format!("{label}:") };

    let (eta_str, wait_str) = if over {
        let eta = pace
            .and_then(|p| p.depletion_eta.as_ref())
            .map(|eta| {
                let local: chrono::DateTime<chrono::Local> = eta.with_timezone(&chrono::Local);
                let today = chrono::Local::now().date_naive();
                let fmt = if local.date_naive() == today {
                    local.format("%-H:%M").to_string()
                } else {
                    local.format("%-m/%-d %-H:%M").to_string()
                };
                format!(" →{fmt}")
            })
            .unwrap_or_default();
        let wait = pace
            .and_then(|p| p.recovery_secs)
            .map(|secs| format!(" ↓{}", format_short_duration(secs)))
            .unwrap_or_default();
        (eta, wait)
    } else {
        (String::new(), String::new())
    };

    let text = format!(
        "{prefix}{:.0}/{:.0}%{alert}{eta_str}{wait_str}",
        w.used_percent, pace_pct
    );
    block(bg, fg, &text)
}

/// 紧凑时长：<1m→"1m"，<1h→"Xm"，<1d→"XhYm"（省空格）, >=1d→"XdYh"
fn format_short_duration(secs: i64) -> String {
    if secs < 60 {
        "1m".to_string()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 { format!("{h}h") } else { format!("{h}h{m}m") }
    } else {
        let d = secs / 86400;
        let h = (secs % 86400) / 3600;
        if h == 0 { format!("{d}d") } else { format!("{d}d{h}h") }
    }
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
    let project_name_raw = ctx
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
    let project_name_clean = crate::input::sanitize_external(project_name_raw);
    identity.push(block(
        BG_PROJECT,
        FG_DARK,
        &truncate_visual(&project_name_clean, 16),
    ));

    // git 段：branch[*][↑N][↓M]，branch 剥控制字符 + 截断到 16 列
    if let Some(branch) = &ctx.git.branch {
        let branch_clean = crate::input::sanitize_external(branch);
        let branch_short = truncate_visual(&branch_clean, 16);
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

    // 代码改动量块：+X 绿 / -Y 红，中性灰底
    if ctx.config.display.edit_stats {
        if let Some((added, removed)) = edit_stats_parts(&ctx.stdin) {
            identity.push(stats_block(&added, &removed));
        }
    }

    let t = &ctx.config.thresholds;

    // ctx
    if ctx.config.display.context {
        let ctx_pct = crate::input::get_context_percent(&ctx.stdin);
        let (bg, fg) = ctx_block_colors(ctx_pct, t);
        metrics.push(block(bg, fg, &format!("ctx {ctx_pct:.0}%")));
    }

    // 5h
    if ctx.config.display.five_hour {
        if let Some(w) = &ctx.usage.five_hour {
            let pace = crate::usage::calc_pace(w, crate::usage::WINDOW_5H_SECS, t.pace_tolerance);
            metrics.push(quota_block(
                w,
                pace.as_ref(),
                "",
                t.five_hour_yellow_at,
                t.five_hour_red_at,
            ));
        }
    }

    // 7d
    if ctx.config.display.seven_day {
        if let Some(w) = &ctx.usage.seven_day {
            let pace = crate::usage::calc_pace(w, crate::usage::WINDOW_7D_SECS, t.pace_tolerance);
            metrics.push(quota_block(
                w,
                pace.as_ref(),
                "",
                t.seven_day_yellow_at,
                t.seven_day_red_at,
            ));
        }
    }

    // Sonnet 7d（仅超速时出现）
    if ctx.config.display.seven_day {
        if let Some(w) = &ctx.usage.seven_day_sonnet {
            let pace = crate::usage::calc_pace(w, crate::usage::WINDOW_7D_SECS, t.pace_tolerance);
            if pace.as_ref().is_some_and(|p| p.direction == crate::usage::PaceDirection::Over) {
                metrics.push(quota_block(
                    w,
                    pace.as_ref(),
                    "S",
                    t.seven_day_yellow_at,
                    t.seven_day_red_at,
                ));
            }
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
