use crate::input::RateLimits;
use chrono::{DateTime, Utc};
use serde::Deserialize;

// ── Usage 数据（rate_limits + cache + API fallback + pace） ──

#[derive(Debug, Clone)]
pub struct UsageData {
    pub five_hour: Option<WindowUsage>,
    pub seven_day: Option<WindowUsage>,
    /// Sonnet 专属 7d 额度（来自 API 的 seven_day_sonnet 字段）
    pub seven_day_sonnet: Option<WindowUsage>,
    /// Opus 专属 7d 额度（来自 API 的 seven_day_opus 字段；未跑 Opus 时 API 返回 null）
    pub seven_day_opus: Option<WindowUsage>,
    /// 月度付费扩容池（按需开启；is_enabled=false 或 API 缺失时为 None）
    pub extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Clone)]
pub struct WindowUsage {
    /// 已使用百分比 0-100
    pub used_percent: f64,
    /// 重置时间（UTC ISO 字符串）
    pub resets_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 月度付费扩容池数据（来自 API extra_usage 节点）
#[derive(Debug, Clone)]
pub struct ExtraUsage {
    /// 月度上限（货币单位由 currency 决定，目前观测仅 "USD"）
    pub monthly_limit: f64,
    /// 已用额度
    pub used_credits: f64,
    /// 利用率 0-100；与 used/limit 之比一致，由 API 直接给出
    pub utilization: f64,
    /// 货币代码（"USD" 等）
    pub currency: String,
}

/// 配速数据
#[derive(Debug)]
pub struct PaceInfo {
    /// 配速位置百分比 0-100（时间窗口已过比例）
    pub pace_percent: f64,
    /// 方向指示：>0 超速, <0 低速, 0 正常
    pub direction: PaceDirection,
    /// 按当前消耗速率预估的耗尽时间（仅当耗尽时间早于窗口重置时有值）
    pub depletion_eta: Option<chrono::DateTime<chrono::Utc>>,
    /// 停工恢复时间（秒）：停止使用后多久配速能追平当前用量
    pub recovery_secs: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub enum PaceDirection {
    /// used > pace（无容差，配速线被超过即触发）
    Over,
    /// used < pace
    Under,
    /// used == pace
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
    #[serde(default)]
    seven_day_sonnet_pct: Option<f64>,
    #[serde(default)]
    seven_day_sonnet_resets_at: Option<String>,
    #[serde(default)]
    seven_day_opus_pct: Option<f64>,
    #[serde(default)]
    seven_day_opus_resets_at: Option<String>,
    #[serde(default)]
    extra_usage: Option<CachedExtraUsage>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CachedExtraUsage {
    monthly_limit: f64,
    used_credits: f64,
    utilization: f64,
    currency: String,
}

// ── API 响应结构 ──

#[derive(Debug, Deserialize)]
struct ApiUsageResponse {
    five_hour: Option<ApiWindow>,
    seven_day: Option<ApiWindow>,
    seven_day_sonnet: Option<ApiWindow>,
    #[serde(default)]
    seven_day_opus: Option<ApiWindow>,
    #[serde(default)]
    extra_usage: Option<ApiExtraUsage>,
}

#[derive(Debug, Deserialize)]
struct ApiWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiExtraUsage {
    /// API 用 is_enabled 标记扩容池是否激活；false 时其他字段无意义
    #[serde(default)]
    is_enabled: bool,
    #[serde(default)]
    monthly_limit: Option<f64>,
    #[serde(default)]
    used_credits: Option<f64>,
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default)]
    currency: Option<String>,
}

// ── 辅助：解析 resets_at（兼容 Unix 时间戳和 ISO 字符串） ──

fn parse_resets_at(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    match value {
        serde_json::Value::Number(n) => {
            let ts = n.as_i64()?;
            DateTime::from_timestamp(ts, 0)
        }
        serde_json::Value::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc)),
        _ => None,
    }
}

// ── 常量 ──

pub const WINDOW_5H_SECS: i64 = 5 * 3600;
pub const WINDOW_7D_SECS: i64 = 7 * 24 * 3600;
const CACHE_TTL_SUCCESS: i64 = 5 * 60;

// ── 公共函数 ──

fn debug_enabled() -> bool {
    std::env::var("LIFELINE_DEBUG").map(|v| v == "1").unwrap_or(false)
}

/// 从 stdin rate_limits + cache + API fallback 获取 usage 数据
pub async fn get_usage_data(rate_limits: Option<&RateLimits>) -> UsageData {
    // 优先级 1: stdin rate_limits
    if let Some(rl) = rate_limits {
        if debug_enabled() {
            eprintln!(
                "[lifeline] rate_limits raw: 5h={:?} 7d={:?}",
                rl.five_hour.as_ref().map(|w| w.used_percentage),
                rl.seven_day.as_ref().map(|w| w.used_percentage),
            );
        }
        let five_hour = rl.five_hour.as_ref().map(|w| WindowUsage {
            used_percent: w.used_percentage.unwrap_or(0.0),
            resets_at: w.resets_at.as_ref().and_then(parse_resets_at),
        });
        let seven_day = rl.seven_day.as_ref().map(|w| WindowUsage {
            used_percent: w.used_percentage.unwrap_or(0.0),
            resets_at: w.resets_at.as_ref().and_then(parse_resets_at),
        });

        let has_data = five_hour.is_some() || seven_day.is_some();
        if has_data {
            // rate_limits 无 per-model / extra_usage 字段，保留缓存里已有的数据。
            // 每个子字段的有效性独立判定 —— 不能因 5h 窗口刚 reset 就丢掉仍然新鲜的副窗口
            let cache = read_cache().await.filter(cache_within_ttl);
            let seven_day_sonnet = cache.as_ref().and_then(|c| sonnet_from_cache(&c.data));
            let seven_day_opus = cache.as_ref().and_then(|c| opus_from_cache(&c.data));
            let extra_usage = cache
                .as_ref()
                .and_then(|c| c.data.extra_usage.as_ref().map(extra_from_cache));
            let usage = UsageData {
                five_hour,
                seven_day,
                seven_day_sonnet,
                seven_day_opus,
                extra_usage,
            };
            write_cache(&usage).await;
            return usage;
        }
    }

    // 优先级 2: 缓存文件 —— 仅看 cache 整体 TTL；每个窗口的 resets_at 在
    // cached_to_usage 内部独立过滤，避免一个窗口刚 reset 拖累其他窗口
    if let Some(cache) = read_cache().await {
        if cache_within_ttl(&cache) {
            let usage = cached_to_usage(&cache.data);
            return usage;
        }
    }

    // 优先级 3: API fallback
    if let Some(usage) = fetch_usage_from_api().await {
        write_cache(&usage).await;
        return usage;
    }

    // 优先级 4: 无数据
    UsageData {
        five_hour: None,
        seven_day: None,
        seven_day_sonnet: None,
        seven_day_opus: None,
        extra_usage: None,
    }
}

/// 计算配速信息（tolerance 为超速容差百分比，used > pace + tolerance 才算超速）
pub fn calc_pace(window: &WindowUsage, window_secs: i64, tolerance: f64) -> Option<PaceInfo> {
    let resets_at = window.resets_at.as_ref()?;
    let now = Utc::now();
    let remaining_secs = (*resets_at - now).num_seconds();
    let elapsed_secs = window_secs - remaining_secs;

    let pace_percent = ((elapsed_secs as f64 / window_secs as f64) * 100.0).clamp(0.0, 100.0);

    let direction = if window.used_percent > pace_percent + tolerance {
        PaceDirection::Over
    } else if window.used_percent < pace_percent {
        PaceDirection::Under
    } else {
        PaceDirection::Normal
    };

    // 耗尽时间预估：用量超出配速才有意义
    let depletion_eta = if window.used_percent > 5.0
        && elapsed_secs > 60
        && window.used_percent > pace_percent
    {
        let burn_rate = window.used_percent / elapsed_secs as f64;
        if burn_rate > 0.0 {
            let secs_to_100 = ((100.0 - window.used_percent) / burn_rate) as i64;
            if secs_to_100 > 0 {
                let eta = now + chrono::Duration::seconds(secs_to_100);
                if eta < *resets_at { Some(eta) } else { None }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // 恢复时间：超速时停工多久可以让配速追平用量
    // recovery_secs = (used - pace) / (100 / window_secs) = (used - pace) * window_secs / 100
    // 保底 1 秒：direction==Over 时永远返回 Some，和 `!` 标记保持一致
    // （否则亚百分点超速会走 truncation → 0 → None，用户只见 `!` 不见 `↓`，语义打架）
    let recovery_secs = if direction == PaceDirection::Over {
        let delta = window.used_percent - pace_percent;
        let secs = (delta * window_secs as f64 / 100.0) as i64;
        Some(secs.max(1))
    } else {
        None
    };

    if debug_enabled() {
        eprintln!(
            "[lifeline] calc_pace: used={:.2} pace={:.2} dir={:?} eta={:?} recovery={:?}s",
            window.used_percent, pace_percent, direction, depletion_eta, recovery_secs
        );
    }

    Some(PaceInfo {
        pace_percent,
        direction,
        depletion_eta,
        recovery_secs,
    })
}

// ── 私有辅助函数 ──

/// 缓存文件路径: ~/.claude/claude-lifeline/usage-cache.json
fn cache_path() -> std::path::PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home)
        .join(".claude")
        .join("claude-lifeline")
        .join("usage-cache.json")
}

/// 读取缓存文件（async，限制 128 KiB 防 symlink 攻击）
async fn read_cache() -> Option<CacheFile> {
    use tokio::io::AsyncReadExt;
    let path = cache_path();
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut buf = String::new();
    file.take(128 * 1024).read_to_string(&mut buf).await.ok()?;
    serde_json::from_str(&buf).ok()
}

/// 写入缓存文件（async tokio::fs，避免阻塞 reactor 线程）
async fn write_cache(data: &UsageData) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    let cached = CachedUsage {
        five_hour_pct: data.five_hour.as_ref().map(|w| w.used_percent),
        five_hour_resets_at: data
            .five_hour
            .as_ref()
            .and_then(|w| w.resets_at.map(|dt| dt.to_rfc3339())),
        seven_day_pct: data.seven_day.as_ref().map(|w| w.used_percent),
        seven_day_resets_at: data
            .seven_day
            .as_ref()
            .and_then(|w| w.resets_at.map(|dt| dt.to_rfc3339())),
        seven_day_sonnet_pct: data.seven_day_sonnet.as_ref().map(|w| w.used_percent),
        seven_day_sonnet_resets_at: data
            .seven_day_sonnet
            .as_ref()
            .and_then(|w| w.resets_at.map(|dt| dt.to_rfc3339())),
        seven_day_opus_pct: data.seven_day_opus.as_ref().map(|w| w.used_percent),
        seven_day_opus_resets_at: data
            .seven_day_opus
            .as_ref()
            .and_then(|w| w.resets_at.map(|dt| dt.to_rfc3339())),
        extra_usage: data.extra_usage.as_ref().map(|e| CachedExtraUsage {
            monthly_limit: e.monthly_limit,
            used_credits: e.used_credits,
            utilization: e.utilization,
            currency: e.currency.clone(),
        }),
    };

    let cache_file = CacheFile {
        data: cached,
        timestamp: Utc::now().timestamp(),
    };

    if let Ok(json) = serde_json::to_string(&cache_file) {
        let _ = tokio::fs::write(path, json).await;
    }
}

/// cache 是否在整体 TTL 内（不再做"任一窗口 reset 就整个失效"的激进判定 ——
/// 单个窗口的 resets_at 在 cached_to_usage 内部独立过滤）
fn cache_within_ttl(cache: &CacheFile) -> bool {
    let now = Utc::now().timestamp();
    now - cache.timestamp < CACHE_TTL_SUCCESS
}

/// 把单个窗口的 (pct, resets_at_str) 转 WindowUsage，过滤已过期窗口
fn window_from_cache(pct: Option<f64>, resets_at: Option<&str>) -> Option<WindowUsage> {
    let pct = pct?;
    let resets_at = resets_at
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    // resets_at 已过期 → 该窗口刚 reset，旧数据无意义
    if let Some(ts) = resets_at {
        if ts < Utc::now() {
            return None;
        }
    }
    Some(WindowUsage { used_percent: pct, resets_at })
}

fn sonnet_from_cache(cached: &CachedUsage) -> Option<WindowUsage> {
    window_from_cache(
        cached.seven_day_sonnet_pct,
        cached.seven_day_sonnet_resets_at.as_deref(),
    )
}

fn opus_from_cache(cached: &CachedUsage) -> Option<WindowUsage> {
    window_from_cache(
        cached.seven_day_opus_pct,
        cached.seven_day_opus_resets_at.as_deref(),
    )
}

fn extra_from_cache(cached: &CachedExtraUsage) -> ExtraUsage {
    ExtraUsage {
        monthly_limit: cached.monthly_limit,
        used_credits: cached.used_credits,
        utilization: cached.utilization,
        currency: cached.currency.clone(),
    }
}

/// 从 CachedUsage 转换为 UsageData
fn cached_to_usage(cached: &CachedUsage) -> UsageData {
    UsageData {
        five_hour: window_from_cache(
            cached.five_hour_pct,
            cached.five_hour_resets_at.as_deref(),
        ),
        seven_day: window_from_cache(
            cached.seven_day_pct,
            cached.seven_day_resets_at.as_deref(),
        ),
        seven_day_sonnet: sonnet_from_cache(cached),
        seven_day_opus: opus_from_cache(cached),
        extra_usage: cached.extra_usage.as_ref().map(extra_from_cache),
    }
}

/// 通过 API 获取 usage 数据
async fn fetch_usage_from_api() -> Option<UsageData> {
    let cred = crate::auth::read_credentials()?;
    let token = cred.access_token.as_deref()?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;

    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", "claude-code/2.1")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let api_resp: ApiUsageResponse = resp.json().await.ok()?;

    let five_hour = api_resp.five_hour.map(|w| WindowUsage {
        used_percent: w.utilization.unwrap_or(0.0),
        resets_at: w
            .resets_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
    });

    let seven_day = api_resp.seven_day.map(|w| WindowUsage {
        used_percent: w.utilization.unwrap_or(0.0),
        resets_at: w
            .resets_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
    });

    let seven_day_sonnet = api_resp.seven_day_sonnet.map(|w| WindowUsage {
        used_percent: w.utilization.unwrap_or(0.0),
        resets_at: w
            .resets_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
    });

    let seven_day_opus = api_resp.seven_day_opus.map(|w| WindowUsage {
        used_percent: w.utilization.unwrap_or(0.0),
        resets_at: w
            .resets_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
    });

    let extra_usage = api_resp.extra_usage.and_then(|e| {
        // is_enabled=false 时 monthly_limit/used_credits 字段可能仍存在，
        // 但语义上扩容池没开通；丢弃以避免误展示
        if !e.is_enabled {
            return None;
        }
        Some(ExtraUsage {
            monthly_limit: e.monthly_limit.unwrap_or(0.0),
            used_credits: e.used_credits.unwrap_or(0.0),
            utilization: e.utilization.unwrap_or(0.0),
            currency: e.currency.unwrap_or_else(|| "USD".to_string()),
        })
    });

    Some(UsageData {
        five_hour,
        seven_day,
        seven_day_sonnet,
        seven_day_opus,
        extra_usage,
    })
}
