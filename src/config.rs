use serde::Deserialize;

/// 用户配置（~/.claude/claude-lifeline/config.toml）
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "DisplayConfig::default")]
    pub display: DisplayConfig,
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
    /// 显示顶部分割线
    #[serde(default = "yes")]
    pub separator: bool,
}

fn yes() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        Self { display: DisplayConfig::default() }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            context: true,
            five_hour: true,
            seven_day: true,
            separator: true,
        }
    }
}

/// 读取配置文件，不存在或解析失败时返回默认值
pub fn read_config() -> Config {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let path = std::path::PathBuf::from(home)
        .join(".claude")
        .join("claude-lifeline")
        .join("config.toml");

    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}
