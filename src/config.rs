use serde::Deserialize;

/// 用户配置（~/.claude/claude-lifeline/config.toml）
#[derive(Debug, Deserialize)]
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
    /// ctx >= 该值：standard 模式显示 token 明细 (in:Xk c:Yk)
    #[serde(default = "d_ctx_token_detail")]
    pub ctx_token_detail_at: f64,
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
fn d_ctx_token_detail() -> f64 { 85.0 }
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
            ctx_token_detail_at: d_ctx_token_detail(),
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
        if !(0.0..=100.0).contains(&self.ctx_token_detail_at) {
            self.ctx_token_detail_at = def.ctx_token_detail_at;
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

#[derive(Debug, Deserialize)]
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
    /// 布局：auto 按终端宽度自动拆分，single 强制单行，multi 强制每段独占一行，mini 极简色块单行
    #[serde(default = "Layout::default")]
    pub layout: Layout,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Auto,
    Single,
    Multi,
    Mini,
}

impl Layout {
    fn default() -> Self { Self::Auto }
}

fn yes() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        Self {
            display: DisplayConfig::default(),
            thresholds: Thresholds::default(),
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            context: true,
            five_hour: true,
            seven_day: true,
            edit_stats: true,
            layout: Layout::Auto,
        }
    }
}

/// 读取配置文件，不存在或解析失败时返回默认值；阈值字段超出范围自动回退
pub fn read_config() -> Config {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let path = std::path::PathBuf::from(home)
        .join(".claude")
        .join("claude-lifeline")
        .join("config.toml");

    let mut cfg: Config = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    cfg.thresholds = cfg.thresholds.sanitize();
    cfg
}
