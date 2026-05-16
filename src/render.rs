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

/// Emit a JSON snapshot of the same data the ANSI renderer would draw. Stable schema —
/// callers should parse `schema_version` and treat unknown fields as additive.
pub fn render_json(ctx: &RenderContext) {
    let value = crate::json_render::build(ctx);
    match serde_json::to_string_pretty(&value) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("{{}}"),
    }
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
const BG_SUBSCRIPTION: u8 = 60; // #5f5f87 紫灰，订阅块；与 Sonnet 99 / Opus 134 拉开

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

/// 扩容池金额紧凑显示：>=1000 用 1.2K 风格，<1000 取整。
/// 非有限值（NaN / ±Inf）统一降级为 `?`，防御 API 异常返回污染 UI
fn format_credits(amount: f64) -> String {
    if !amount.is_finite() {
        return "?".to_string();
    }
    let abs = amount.abs();
    if abs >= 10_000.0 {
        format!("{:.0}K", amount / 1_000.0)
    } else if abs >= 1_000.0 {
        format!("{:.1}K", amount / 1_000.0)
    } else {
        format!("{amount:.0}")
    }
}

/// 货币代码的最大显示长度。ISO 4217 是 3 字符；放宽到 6 容纳非标代号，
/// 同时挡住 API 异常返回（如几十/几百字节）撑爆 prefix
const CURRENCY_MAX_CHARS: usize = 6;

/// 清理货币代码：仅保留 ASCII 字母数字 + 限长。
/// ISO 4217 是 3 个 ASCII 大写字母，放宽到 alphanumeric 容纳少数非标代号；
/// 拒绝括号 / 控制字符 / 标点等任何可能扰乱 `[XXX]` 包装或终端渲染的字符
fn sanitize_currency(raw: &str) -> String {
    let filtered: String = raw.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if filtered.chars().count() <= CURRENCY_MAX_CHARS {
        filtered
    } else {
        filtered.chars().take(CURRENCY_MAX_CHARS).collect()
    }
}

/// extra_usage 块：`$5.4K/20K 27%`；货币非 USD 时降级为 `[XYZ] used/limit pct%`。
/// 颜色阈值有意复用 7d quota 的 yellow_at/red_at —— extra_usage 是月度池，
/// 但语义上同属"长周期累计配额"，独立阈值边际收益有限，先共用避免配置膨胀
fn extra_usage_block(extra: &crate::usage::ExtraUsage, t: &Thresholds) -> String {
    // utilization 进 quota_block_colors 前 clamp，防御 NaN / 越界
    let util = if extra.utilization.is_finite() {
        extra.utilization.clamp(0.0, 100.0)
    } else {
        0.0
    };
    let (bg, fg) = quota_block_colors(util, false, t.seven_day_yellow_at, t.seven_day_red_at);
    let used = format_credits(extra.used_credits);
    let limit = format_credits(extra.monthly_limit);
    let currency = sanitize_currency(&extra.currency);
    let prefix = if currency.eq_ignore_ascii_case("USD") {
        "$".to_string()
    } else if currency.is_empty() {
        // 极端情况：API 返回空 currency 又非 USD 默认值；不画 prefix 避免 `[] xxx`
        String::new()
    } else {
        format!("[{currency}] ")
    };
    let text = format!("{prefix}{used}/{limit} {util:.0}%");
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

    // 订阅块：从 OAuth 凭证里解析，紧贴 model；凭证缺失/解析失败时不渲染
    // 调用走 read_credentials —— 凭证文件已被 usage 模块读过一次，但 OS 一般会缓存 inode，
    // 二次同步读 <1ms 不影响 sub-50ms 目标。Keychain fallback 仅在凭证文件缺失时触发
    if ctx.config.display.subscription {
        if let Some(cred) = crate::auth::read_credentials() {
            if let Some(label) = crate::auth::subscription_label(
                cred.subscription_type.as_deref(),
                cred.rate_limit_tier.as_deref(),
            ) {
                identity.push(block(BG_SUBSCRIPTION, FG_DARK, &label));
            }
        }
    }

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

    // Opus 7d（仅 display.seven_day_opus 开启 + 超速时出现，行为对齐 Sonnet）
    if ctx.config.display.seven_day_opus {
        if let Some(w) = &ctx.usage.seven_day_opus {
            let pace = crate::usage::calc_pace(w, crate::usage::WINDOW_7D_SECS, t.pace_tolerance);
            if pace.as_ref().is_some_and(|p| p.direction == crate::usage::PaceDirection::Over) {
                metrics.push(quota_block(
                    w,
                    pace.as_ref(),
                    "O",
                    t.seven_day_yellow_at,
                    t.seven_day_red_at,
                ));
            }
        }
    }

    // 扩容池：display 开启 + extra_usage 存在（is_enabled=false 已在 usage 层过滤）才出现
    if ctx.config.display.extra_usage {
        if let Some(extra) = &ctx.usage.extra_usage {
            metrics.push(extra_usage_block(extra, t));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::ExtraUsage;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                if let Some('[') = chars.next() {
                    for c2 in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c2) {
                            break;
                        }
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }

    #[test]
    fn credits_compact_format() {
        assert_eq!(format_credits(0.0), "0");
        assert_eq!(format_credits(123.4), "123");
        assert_eq!(format_credits(999.0), "999");
        assert_eq!(format_credits(1_000.0), "1.0K");
        assert_eq!(format_credits(5_440.0), "5.4K");
        assert_eq!(format_credits(20_000.0), "20K");
        assert_eq!(format_credits(123_456.0), "123K");
    }

    #[test]
    fn extra_usage_block_usd_format() {
        let extra = ExtraUsage {
            monthly_limit: 20_000.0,
            used_credits: 5_440.0,
            utilization: 27.2,
            currency: "USD".into(),
        };
        let t = Thresholds::default();
        let plain = strip_ansi(&extra_usage_block(&extra, &t));
        // 前后各一空格内边距是 block() 的契约
        assert_eq!(plain, " $5.4K/20K 27% ");
    }

    #[test]
    fn extra_usage_block_non_usd_uses_bracket_prefix() {
        let extra = ExtraUsage {
            monthly_limit: 1_500.0,
            used_credits: 450.0,
            utilization: 30.0,
            currency: "EUR".into(),
        };
        let plain = strip_ansi(&extra_usage_block(&extra, &Thresholds::default()));
        assert_eq!(plain, " [EUR] 450/1.5K 30% ");
    }

    #[test]
    fn extra_usage_block_color_thresholds() {
        // 利用率 < yellow_at(80) → 蓝
        let safe = ExtraUsage {
            monthly_limit: 100.0,
            used_credits: 10.0,
            utilization: 10.0,
            currency: "USD".into(),
        };
        let raw = extra_usage_block(&safe, &Thresholds::default());
        assert!(raw.contains(&format!("48;5;{BG_QUOTA_SAFE}")));

        // 利用率 >= red_at(90) → 红
        let danger = ExtraUsage {
            monthly_limit: 100.0,
            used_credits: 95.0,
            utilization: 95.0,
            currency: "USD".into(),
        };
        let raw = extra_usage_block(&danger, &Thresholds::default());
        assert!(raw.contains(&format!("48;5;{BG_DANGER}")));
    }

    #[test]
    fn visible_width_strips_ansi() {
        let s = format!("{}plain{}", "\x1b[48;5;60m", "\x1b[0m");
        assert_eq!(visible_width(&s), "plain".chars().count());
    }

    #[test]
    fn credits_handles_non_finite() {
        assert_eq!(format_credits(f64::NAN), "?");
        assert_eq!(format_credits(f64::INFINITY), "?");
        assert_eq!(format_credits(f64::NEG_INFINITY), "?");
        // 有限值不受影响（回归）
        assert_eq!(format_credits(5_440.0), "5.4K");
    }

    #[test]
    fn extra_usage_block_clamps_utilization_overflow() {
        // API 异常返回 >100% 利用率：clamp 到 100，不让百分比 UI 越界
        let extra = ExtraUsage {
            monthly_limit: 100.0,
            used_credits: 250.0,
            utilization: 250.0,
            currency: "USD".into(),
        };
        let plain = strip_ansi(&extra_usage_block(&extra, &Thresholds::default()));
        assert!(plain.contains("100%"), "got: {plain}");
        assert!(!plain.contains("250%"));
    }

    #[test]
    fn extra_usage_block_handles_nan_utilization() {
        let extra = ExtraUsage {
            monthly_limit: 100.0,
            used_credits: 50.0,
            utilization: f64::NAN,
            currency: "USD".into(),
        };
        let plain = strip_ansi(&extra_usage_block(&extra, &Thresholds::default()));
        // NaN clamp 后置 0%，不再泄漏字面 "NaN"
        assert!(plain.contains("0%"));
        assert!(!plain.contains("NaN"));
    }

    #[test]
    fn extra_usage_block_sanitizes_currency_control_chars() {
        // 防 ANSI 转义注入：构造完整的 SGR 序列作为 currency，
        // 滤掉 ESC + `[` + `;` + `m` 等所有非 alphanumeric 字符后应只剩字母数字
        let extra = ExtraUsage {
            monthly_limit: 100.0,
            used_credits: 10.0,
            utilization: 10.0,
            currency: "AB\x1b[31;1mCD".into(),
        };
        let raw = extra_usage_block(&extra, &Thresholds::default());
        // 关键安全断言：原始 ESC 字节不得出现在 block() 包装之外的 SGR 序列里
        // block() 自己的 SGR 是 `\x1b[48;5;...` 和 `\x1b[0m`，不会含 `\x1b[31;1m`
        assert!(
            !raw.contains("\x1b[31"),
            "ESC injection survived sanitization: {raw:?}"
        );
        // 滤后仅保留 alphanumeric：`AB[31;1mCD` 里的 `[`/`;`/`\x1b` 全删，留 `AB311mCD`
        let plain = strip_ansi(&raw);
        assert!(plain.contains("[AB311m"), "got: {plain}");
        // 进一步保证：visible 输出全 ASCII 可打印，无残留控制字符
        for c in plain.chars() {
            assert!(
                !c.is_control(),
                "control char in visible output: {:?} in {plain:?}",
                c
            );
        }
    }

    #[test]
    fn extra_usage_block_truncates_oversized_currency() {
        // 防御 API 返回数百字节 currency 撑爆 UI
        let huge = "X".repeat(500);
        let extra = ExtraUsage {
            monthly_limit: 100.0,
            used_credits: 10.0,
            utilization: 10.0,
            currency: huge,
        };
        let plain = strip_ansi(&extra_usage_block(&extra, &Thresholds::default()));
        // currency 部分被截到 6 字符以内（CURRENCY_MAX_CHARS）
        assert!(plain.contains("[XXXXXX]"), "got: {plain}");
        assert!(!plain.contains("XXXXXXX"), "got: {plain}");
    }

    #[test]
    fn extra_usage_block_empty_currency_omits_prefix() {
        let extra = ExtraUsage {
            monthly_limit: 100.0,
            used_credits: 10.0,
            utilization: 10.0,
            currency: String::new(),
        };
        let plain = strip_ansi(&extra_usage_block(&extra, &Thresholds::default()));
        assert!(!plain.contains("[]"));
        assert!(!plain.contains('$'));
    }
}
