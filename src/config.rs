use serde::Deserialize;

/// 用户配置（~/.claude/claude-lifeline/config.toml）
#[derive(Debug, Default, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "DisplayConfig::default")]
    pub display: DisplayConfig,
    #[serde(default = "Thresholds::default")]
    pub thresholds: Thresholds,
}

/// 颜色切换阈值（用户可覆盖；mini & standard 共用）
#[derive(Debug, Deserialize, Clone, Copy)]
pub struct Thresholds {
    /// ctx >= 该值：绿 → 黄
    #[serde(default = "d_ctx_yellow")]
    pub ctx_yellow_at: f64,
    /// ctx >= 该值：黄 → 红
    #[serde(default = "d_ctx_red")]
    pub ctx_red_at: f64,
    /// 5h quota >= 该值（或超速）：蓝 → 黄
    #[serde(default = "d_5h_yellow")]
    pub five_hour_yellow_at: f64,
    /// 5h quota >= 该值：黄 → 红
    #[serde(default = "d_5h_red")]
    pub five_hour_red_at: f64,
    /// 7d quota >= 该值（或超速）：蓝 → 黄
    #[serde(default = "d_7d_yellow")]
    pub seven_day_yellow_at: f64,
    /// 7d quota >= 该值：黄 → 红
    #[serde(default = "d_7d_red")]
    pub seven_day_red_at: f64,
    /// 配速容差（%）：used > pace + tolerance 才算超速；0 = 严格模式
    #[serde(default = "d_pace_tolerance")]
    pub pace_tolerance: f64,
}

fn d_ctx_yellow() -> f64 { 60.0 }
fn d_ctx_red() -> f64 { 70.0 }
fn d_5h_yellow() -> f64 { 75.0 }
fn d_5h_red() -> f64 { 90.0 }
fn d_7d_yellow() -> f64 { 80.0 }
fn d_7d_red() -> f64 { 90.0 }
fn d_pace_tolerance() -> f64 { 0.0 }

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            ctx_yellow_at: d_ctx_yellow(),
            ctx_red_at: d_ctx_red(),
            five_hour_yellow_at: d_5h_yellow(),
            five_hour_red_at: d_5h_red(),
            seven_day_yellow_at: d_7d_yellow(),
            seven_day_red_at: d_7d_red(),
            pace_tolerance: d_pace_tolerance(),
        }
    }
}

impl Thresholds {
    /// 校验：值落在 [0, 100]，且 yellow < red。不合法的字段对（yellow/red）单独回退默认
    pub fn sanitize(mut self) -> Self {
        let def = Self::default();
        for (pair_valid, yellow, red, dy, dr) in [
            (yellow_before_red(self.ctx_yellow_at, self.ctx_red_at),
             &mut self.ctx_yellow_at, &mut self.ctx_red_at, def.ctx_yellow_at, def.ctx_red_at),
            (yellow_before_red(self.five_hour_yellow_at, self.five_hour_red_at),
             &mut self.five_hour_yellow_at, &mut self.five_hour_red_at, def.five_hour_yellow_at, def.five_hour_red_at),
            (yellow_before_red(self.seven_day_yellow_at, self.seven_day_red_at),
             &mut self.seven_day_yellow_at, &mut self.seven_day_red_at, def.seven_day_yellow_at, def.seven_day_red_at),
        ] {
            if !pair_valid {
                *yellow = dy;
                *red = dr;
            }
        }
        if !(0.0..=100.0).contains(&self.pace_tolerance) {
            self.pace_tolerance = def.pace_tolerance;
        }
        self
    }
}

fn yellow_before_red(y: f64, r: f64) -> bool {
    (0.0..=100.0).contains(&y) && (0.0..=100.0).contains(&r) && y < r
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct DisplayConfig {
    /// 显示 context window 段
    #[serde(default = "yes")]
    pub context: bool,
    /// 显示 5h quota 段
    #[serde(default = "yes")]
    pub five_hour: bool,
    /// 显示 7d quota 段
    #[serde(default = "yes")]
    pub seven_day: bool,
    /// 显示代码改动量 +X -Y（仅当本 session 有增删时）
    #[serde(default = "yes")]
    pub edit_stats: bool,
    /// 显示订阅类型块（如 `MAX·20x` / `PRO`）。默认关闭，保持升级后 UI 不变
    #[serde(default = "no")]
    pub subscription: bool,
    /// 显示 Opus 7d 子额度块（类比 Sonnet：仅超速时出现）。默认关闭
    #[serde(default = "no")]
    pub seven_day_opus: bool,
    /// 显示月度付费扩容池块（仅 is_enabled=true 时有意义）。默认关闭
    #[serde(default = "no")]
    pub extra_usage: bool,
    /// 显示会话累计费用（$X.XX）。默认关闭
    #[serde(default = "no")]
    pub session_cost: bool,
}

fn yes() -> bool { true }
fn no() -> bool { false }

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            context: true,
            five_hour: true,
            seven_day: true,
            edit_stats: true,
            subscription: false,
            seven_day_opus: false,
            extra_usage: false,
            session_cost: false,
        }
    }
}

/// Stable identifier for TUI display toggles. Order is the display order.
#[derive(Debug, Clone, Copy)]
pub enum DisplayKey {
    Context,
    FiveHour,
    SevenDay,
    EditStats,
    Subscription,
    SevenDayOpus,
    ExtraUsage,
    SessionCost,
}

impl DisplayKey {
    pub const ALL: [DisplayKey; 8] = [
        DisplayKey::Context,
        DisplayKey::FiveHour,
        DisplayKey::SevenDay,
        DisplayKey::EditStats,
        DisplayKey::Subscription,
        DisplayKey::SevenDayOpus,
        DisplayKey::ExtraUsage,
        DisplayKey::SessionCost,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DisplayKey::Context => "context window",
            DisplayKey::FiveHour => "5h quota",
            DisplayKey::SevenDay => "7d quota (incl. Sonnet sub-block)",
            DisplayKey::EditStats => "edit stats (+/-)",
            DisplayKey::Subscription => "subscription tier badge",
            DisplayKey::SevenDayOpus => "Opus 7d sub-block",
            DisplayKey::ExtraUsage => "extra-usage credits",
            DisplayKey::SessionCost => "session cost ($)",
        }
    }

    pub fn get(&self, cfg: &DisplayConfig) -> bool {
        match self {
            DisplayKey::Context => cfg.context,
            DisplayKey::FiveHour => cfg.five_hour,
            DisplayKey::SevenDay => cfg.seven_day,
            DisplayKey::EditStats => cfg.edit_stats,
            DisplayKey::Subscription => cfg.subscription,
            DisplayKey::SevenDayOpus => cfg.seven_day_opus,
            DisplayKey::ExtraUsage => cfg.extra_usage,
            DisplayKey::SessionCost => cfg.session_cost,
        }
    }

    pub fn set(&self, cfg: &mut DisplayConfig, val: bool) {
        match self {
            DisplayKey::Context => cfg.context = val,
            DisplayKey::FiveHour => cfg.five_hour = val,
            DisplayKey::SevenDay => cfg.seven_day = val,
            DisplayKey::EditStats => cfg.edit_stats = val,
            DisplayKey::Subscription => cfg.subscription = val,
            DisplayKey::SevenDayOpus => cfg.seven_day_opus = val,
            DisplayKey::ExtraUsage => cfg.extra_usage = val,
            DisplayKey::SessionCost => cfg.session_cost = val,
        }
    }
}

/// 读取配置文件，不存在或解析失败时返回默认值；阈值字段超出范围自动回退
pub fn read_config() -> Config {
    let path = crate::data::paths::config_path();
    // 限制 128 KiB：防 symlink 到大文件吞内存（真实配置实测 <2 KiB）
    let mut cfg: Config = read_capped(&path, 128 * 1024)
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    cfg.thresholds = cfg.thresholds.sanitize();
    cfg
}

fn read_capped(path: &std::path::Path, max_bytes: u64) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = String::new();
    f.by_ref().take(max_bytes).read_to_string(&mut buf).ok()?;
    Some(buf)
}

/// Persist the entire config to disk. Used by the TUI write path.
pub fn write_config(config: &Config) -> anyhow::Result<()> {
    let path = crate::data::paths::config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = render_config_toml(config);
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Convenience wrapper: load current config, swap display, write back.
/// Keeps the user's thresholds intact even when the TUI only edits toggles.
pub fn write_display(display: &DisplayConfig) -> anyhow::Result<()> {
    let mut current = read_config();
    current.display = *display;
    write_config(&current)
}

fn render_config_toml(config: &Config) -> String {
    let mut s = String::new();
    s.push_str("# claude-lifeline config — generated, edit by hand or via TUI\n\n");
    s.push_str("[display]\n");
    s.push_str(&format!("context        = {}\n", config.display.context));
    s.push_str(&format!("five_hour      = {}\n", config.display.five_hour));
    s.push_str(&format!("seven_day      = {}\n", config.display.seven_day));
    s.push_str(&format!("edit_stats     = {}\n", config.display.edit_stats));
    s.push_str(&format!("subscription   = {}\n", config.display.subscription));
    s.push_str(&format!("seven_day_opus = {}\n", config.display.seven_day_opus));
    s.push_str(&format!("extra_usage    = {}\n", config.display.extra_usage));
    s.push_str(&format!("session_cost   = {}\n", config.display.session_cost));
    s.push_str("\n[thresholds]\n");
    s.push_str(&format!("ctx_yellow_at       = {}\n", config.thresholds.ctx_yellow_at));
    s.push_str(&format!("ctx_red_at          = {}\n", config.thresholds.ctx_red_at));
    s.push_str(&format!("five_hour_yellow_at = {}\n", config.thresholds.five_hour_yellow_at));
    s.push_str(&format!("five_hour_red_at    = {}\n", config.thresholds.five_hour_red_at));
    s.push_str(&format!("seven_day_yellow_at = {}\n", config.thresholds.seven_day_yellow_at));
    s.push_str(&format!("seven_day_red_at    = {}\n", config.thresholds.seven_day_red_at));
    s.push_str(&format!("pace_tolerance      = {}\n", config.thresholds.pace_tolerance));
    s
}

pub mod cli {
    use crate::cli::ConfigAction;

    pub async fn run(action: ConfigAction) -> anyhow::Result<()> {
        match action {
            ConfigAction::Show => {
                let cfg = super::read_config();
                println!("{}", super::render_config_toml(&cfg));
            }
            ConfigAction::Path => {
                println!("{}", crate::data::paths::config_path().display());
            }
            ConfigAction::Init => {
                let path = crate::data::paths::config_path();
                if path.exists() {
                    println!("{} already exists — not overwriting.", path.display());
                } else {
                    super::write_config(&super::Config::default())?;
                    println!("wrote {}", path.display());
                }
            }
            ConfigAction::Edit => {
                let path = crate::data::paths::config_path();
                if !path.exists() {
                    super::write_config(&super::Config::default())?;
                }
                let editor = std::env::var("VISUAL")
                    .or_else(|_| std::env::var("EDITOR"))
                    .unwrap_or_else(|_| if cfg!(windows) { "notepad".into() } else { "vi".into() });
                let status = std::process::Command::new(&editor)
                    .arg(&path)
                    .status()
                    .map_err(|e| anyhow::anyhow!("failed to launch {editor}: {e}"))?;
                if !status.success() {
                    anyhow::bail!("{editor} exited with {status}");
                }
            }
        }
        Ok(())
    }
}
