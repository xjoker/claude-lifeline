use std::path::PathBuf;

/// 升级检查缓存（24h TTL，不阻塞主流程）
const CHECK_INTERVAL_SECS: i64 = 24 * 3600;
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str = "https://api.github.com/repos/xjoker/claude-lifeline/releases/latest";

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
    crate::data::paths::lifeline_data_root().join("update-cache.json")
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
        touch_cache_sentinel();
        spawn_background_check();
    }

    // 忽略缓存里的 dev/预发布标签，正式版只提示正式版
    if cache.latest_version.contains('-') {
        return None;
    }

    if version_gt(&cache.latest_version, CURRENT_VERSION) {
        let cleaned: String = crate::input::sanitize_external(&cache.latest_version)
            .chars()
            .take(20)
            .collect();
        if cleaned.is_empty() { None } else { Some(cleaned) }
    } else {
        None
    }
}

/// 解析 X.Y.Z（忽略 -suffix 部分）为 (u32, u32, u32) 元组
fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let core = v.split('-').next()?;
    let mut parts = core.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

fn version_gt(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(va), Some(vb)) => va > vb,
        _ => a > b,
    }
}

pub fn ensure_cache_exists() {
    if is_dev_build() {
        return;
    }
    let path = cache_path();
    if !path.exists() {
        touch_cache_sentinel();
        spawn_background_check();
    }
}

fn touch_cache_sentinel() {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let cache = UpdateCache {
        latest_version: CURRENT_VERSION.to_string(),
        checked_at: chrono::Utc::now().timestamp(),
    };
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::write(path, json);
    }
}

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
        .get(RELEASES_API)
        .header("User-Agent", "claude-lifeline")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    let tag = body.get("tag_name")?.as_str()?;
    Some(tag.trim_start_matches('v').to_string())
}

// ── self-update binary replacement ──

#[derive(Debug, serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, serde::Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Hard-cap downloaded asset size — refuse anything beyond this. The release
/// binary is currently a few MiB; cap at 100 MiB to be future-proof while still
/// rejecting a hostile redirect to a multi-GiB tarball.
const ASSET_MAX_BYTES: u64 = 100 * 1024 * 1024;

async fn fetch_release() -> anyhow::Result<Release> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client
        .get(RELEASES_API)
        .header("User-Agent", "claude-lifeline")
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

/// Pick the asset matching the running platform.
fn pick_asset<'a>(release: &'a Release) -> Option<&'a ReleaseAsset> {
    let candidates = platform_asset_patterns();
    release.assets.iter().find(|a| {
        let name = a.name.to_lowercase();
        candidates.iter().any(|p| name.contains(p))
    })
}

fn platform_asset_patterns() -> Vec<&'static str> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    match (os, arch) {
        ("macos", "aarch64") => vec!["darwin-arm64", "darwin-aarch64", "aarch64-apple-darwin", "macos-arm64"],
        ("macos", "x86_64") => vec!["darwin-x86_64", "darwin-amd64", "x86_64-apple-darwin", "macos-x86_64"],
        ("linux", "x86_64") => vec!["linux-x86_64", "linux-amd64", "x86_64-unknown-linux"],
        ("linux", "aarch64") => vec!["linux-aarch64", "linux-arm64", "aarch64-unknown-linux"],
        ("windows", "x86_64") => vec!["windows-x86_64", "windows-amd64", "x86_64-pc-windows", ".exe"],
        _ => vec![],
    }
}

async fn download_to(temp_path: &std::path::Path, asset: &ReleaseAsset) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    if asset.size > ASSET_MAX_BYTES {
        anyhow::bail!(
            "asset {} declares size {} > cap {ASSET_MAX_BYTES}",
            asset.name, asset.size
        );
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let resp = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "claude-lifeline")
        .send()
        .await?
        .error_for_status()?;

    let mut file = tokio::fs::File::create(temp_path).await?;
    let mut stream = resp.bytes_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        written += chunk.len() as u64;
        if written > ASSET_MAX_BYTES {
            anyhow::bail!("download exceeded cap {ASSET_MAX_BYTES} bytes");
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path)?.permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> anyhow::Result<()> {
    Ok(())
}

/// Atomic-ish swap. On Unix `rename` over a running binary works; on Windows
/// we have to rename the current binary aside first.
fn install_binary(downloaded: &std::path::Path, target: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(downloaded, target)?;
        Ok(())
    }
    #[cfg(windows)]
    {
        let backup = target.with_extension("old");
        // Best-effort: ignore failure to remove a stale backup
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(target, &backup)?;
        std::fs::rename(downloaded, target)?;
        Ok(())
    }
}

pub mod cli {
    use crate::cli::UpdateAction;

    pub async fn run(action: UpdateAction) -> anyhow::Result<()> {
        match action {
            UpdateAction::Check => check().await,
            UpdateAction::Run { force } => upgrade(force).await,
        }
    }

    async fn check() -> anyhow::Result<()> {
        let current = super::CURRENT_VERSION;
        let release = super::fetch_release().await?;
        let latest = release.tag_name.trim_start_matches('v');
        if super::version_gt(latest, current) {
            println!("Update available: {current} → {latest}");
            println!("Run `claude-lifeline update run` to upgrade.");
        } else {
            println!("Up to date ({current}).");
        }
        Ok(())
    }

    async fn upgrade(force: bool) -> anyhow::Result<()> {
        let release = super::fetch_release().await?;
        let latest = release.tag_name.trim_start_matches('v').to_string();
        if !force && !super::version_gt(&latest, super::CURRENT_VERSION) {
            println!("Already at {} — pass --force to reinstall.", super::CURRENT_VERSION);
            return Ok(());
        }

        let asset = super::pick_asset(&release).ok_or_else(|| {
            anyhow::anyhow!(
                "no release asset matched platform {}/{} — available: {}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

        println!("Downloading {} ...", asset.name);
        let exe = std::env::current_exe()?;
        let parent = exe.parent().ok_or_else(|| anyhow::anyhow!("cannot resolve parent of current_exe"))?;
        let temp = parent.join(format!(".claude-lifeline.{}.download", std::process::id()));
        super::download_to(&temp, asset).await?;
        super::make_executable(&temp)?;

        println!("Installing → {}", exe.display());
        if let Err(e) = super::install_binary(&temp, &exe) {
            let _ = std::fs::remove_file(&temp);
            return Err(anyhow::anyhow!("install failed: {e}"));
        }

        // Refresh the update cache so the status line stops showing the hint
        let new_cache = super::UpdateCache {
            latest_version: latest.clone(),
            checked_at: chrono::Utc::now().timestamp(),
        };
        if let Ok(json) = serde_json::to_string(&new_cache) {
            let _ = std::fs::write(super::cache_path(), json);
        }

        println!("Upgraded to {latest}.");
        Ok(())
    }
}
