use std::path::PathBuf;

/// 升级检查缓存（24h TTL，不阻塞主流程）

const CHECK_INTERVAL_SECS: i64 = 24 * 3600;
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 本地是否为 dev/预发布构建（版本号含 `-` 后缀，如 `0.0.4-dev`）。
/// dev 构建由开发者自行管理版本，不参与自动更新提示。
fn is_dev_build() -> bool {
    CURRENT_VERSION.contains('-')
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct UpdateCache {
    latest_version: String,
    checked_at: i64,
}

fn cache_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join("claude-lifeline")
        .join("update-cache.json")
}

/// 读取本地缓存，返回新版本号（如果有更新）。纯文件读取，sub-ms。
pub fn check_update_hint() -> Option<String> {
    if is_dev_build() {
        return None;
    }

    let path = cache_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let cache: UpdateCache = serde_json::from_str(&content).ok()?;

    let now = chrono::Utc::now().timestamp();

    // 缓存过期 → 触发后台检查
    if now - cache.checked_at >= CHECK_INTERVAL_SECS {
        spawn_background_check();
    }

    // 忽略缓存里的 dev/预发布标签，正式版只提示正式版
    if cache.latest_version.contains('-') {
        return None;
    }

    // 比较版本（按 SemVer 元组比较，避免 lex 比较把 0.0.10 当成早于 0.0.4）
    if version_gt(&cache.latest_version, CURRENT_VERSION) {
        Some(cache.latest_version)
    } else {
        None
    }
}

/// 解析 X.Y.Z（忽略 -suffix 部分）为 (u32, u32, u32) 元组
fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let core = v.split('-').next()?; // 剥掉 -dev 等后缀
    let mut parts = core.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// 严格大于：a > b。解析失败时回退为字符串比较
fn version_gt(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(va), Some(vb)) => va > vb,
        _ => a > b,
    }
}

/// 首次无缓存时也触发后台检查
pub fn ensure_cache_exists() {
    if is_dev_build() {
        return;
    }
    let path = cache_path();
    if !path.exists() {
        spawn_background_check();
    }
}

/// 派生后台子进程检查更新（不等待结果）
fn spawn_background_check() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("--check-update")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// 实际执行网络检查并写入缓存（由 --check-update 子进程调用）
///
/// 即使网络失败也会写入 cache（latest_version = 当前版本），避免 `ensure_cache_exists`
/// 在离线时反复 spawn 子进程检查更新（每次主进程调用都触发，~300ms 一次）
pub async fn do_update_check() {
    let version = fetch_latest_version()
        .await
        .unwrap_or_else(|| CURRENT_VERSION.to_string());

    let cache = UpdateCache {
        latest_version: version,
        checked_at: chrono::Utc::now().timestamp(),
    };

    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::write(path, json);
    }
}

async fn fetch_latest_version() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let resp = client
        .get("https://api.github.com/repos/xjoker/claude-lifeline/releases/latest")
        .header("User-Agent", "claude-lifeline")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    let tag = body.get("tag_name")?.as_str()?;
    // "v0.0.2" → "0.0.2"
    Some(tag.trim_start_matches('v').to_string())
}
