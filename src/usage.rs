use crate::input::RateLimits;

// ── Usage 数据（rate_limits + cache + API fallback + pace） ──

#[derive(Debug, Clone)]
pub struct UsageData {
    pub five_hour: Option<WindowUsage>,
    pub seven_day: Option<WindowUsage>,
    pub plan_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WindowUsage {
    /// 已使用百分比 0-100
    pub used_percent: f64,
    /// 重置时间（UTC ISO 字符串）
    pub resets_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 配速数据
#[derive(Debug)]
pub struct PaceInfo {
    /// 配速位置百分比 0-100（时间窗口已过比例）
    pub pace_percent: f64,
    /// 方向指示：>0 超速, <0 低速, 0 正常
    pub direction: PaceDirection,
}

#[derive(Debug, PartialEq)]
pub enum PaceDirection {
    /// used > pace + 10%
    Over,
    /// used < pace - 10%
    Under,
    /// 正常范围
    Normal,
}

// ── cache 文件结构 ──

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CacheFile {
    data: CachedUsage,
    timestamp: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CachedUsage {
    five_hour_pct: Option<f64>,
    five_hour_resets_at: Option<String>,
    seven_day_pct: Option<f64>,
    seven_day_resets_at: Option<String>,
    plan_name: Option<String>,
}

// ── 常量 ──

pub const WINDOW_5H_SECS: i64 = 5 * 3600;
pub const WINDOW_7D_SECS: i64 = 7 * 24 * 3600;
const CACHE_TTL_SUCCESS: i64 = 5 * 60;
const CACHE_TTL_FAILURE: i64 = 15;

// ── 公共函数 ──

/// 从 stdin rate_limits + cache + API fallback 获取 usage 数据
pub async fn get_usage_data(rate_limits: Option<&RateLimits>) -> UsageData {
    todo!()
}

/// 计算配速信息
pub fn calc_pace(window: &WindowUsage, window_secs: i64) -> Option<PaceInfo> {
    todo!()
}

/// 格式化重置时间（Xm / Xh Ym / Xd Yh）
pub fn format_reset_time(resets_at: &chrono::DateTime<chrono::Utc>) -> String {
    todo!()
}
