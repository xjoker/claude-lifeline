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
fn pick_asset(release: &Release) -> Option<&ReleaseAsset> {
    let candidates = platform_asset_patterns();
    release.assets.iter().find(|a| {
        let name = a.name.to_lowercase();
        candidates.iter().any(|p| name.contains(p))
    })
}

/// Locate a `SHA256SUMS` asset in the release. Older releases (<0.4.0) lack one —
/// callers handle the `None` case by emitting a warning so the user knows what trust
/// they're extending; mismatch / missing-entry on a release that *does* ship checksums
/// remains a hard abort.
fn find_checksums_asset(release: &Release) -> Option<&ReleaseAsset> {
    release
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("SHA256SUMS"))
}

async fn fetch_checksums(asset: &ReleaseAsset) -> anyhow::Result<String> {
    // Cap at 1 MiB — a real SHA256SUMS for our release matrix is well under 1 KiB;
    // anything larger is either malicious or accidentally pointing at a binary.
    const MAX: u64 = 1_024 * 1_024;
    if asset.size > MAX {
        anyhow::bail!("SHA256SUMS size {} exceeds 1 MiB cap", asset.size);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "claude-lifeline")
        .send()
        .await?
        .error_for_status()?;
    let bytes = resp.bytes().await?;
    if bytes.len() as u64 > MAX {
        anyhow::bail!("SHA256SUMS body exceeded 1 MiB cap");
    }
    Ok(String::from_utf8(bytes.to_vec())?)
}

/// Parse the `sha256sum`-style file and look up the digest for `asset_name`.
/// Each line is `<64-hex-digest>  <filename>` (two spaces between fields). Supports
/// the GNU binary-mode `*filename` form and ignores comment / blank lines.
fn lookup_checksum(checksums: &str, asset_name: &str) -> Option<String> {
    for line in checksums.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let digest = parts.next()?.trim();
        let name = parts.next()?.trim_start_matches('*').trim();
        if name == asset_name && digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(digest.to_ascii_lowercase());
        }
    }
    None
}

/// SHA-256 a file on disk via the `sha2` crate. Buffered to avoid loading the
/// whole binary into RAM.
fn sha256_file(path: &std::path::Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
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
#[cfg(any(windows, test))]
fn replace_with_backup<F>(
    downloaded: &std::path::Path,
    target: &std::path::Path,
    backup: &std::path::Path,
    mut rename: F,
) -> anyhow::Result<()>
where
    F: FnMut(&std::path::Path, &std::path::Path) -> anyhow::Result<()>,
{
    rename(target, backup)?;
    if let Err(install_error) = rename(downloaded, target) {
        return match rename(backup, target) {
            Ok(()) => Err(anyhow::anyhow!(
                "failed to install new binary; restored previous binary: {install_error}"
            )),
            Err(rollback_error) => Err(anyhow::anyhow!(
                "failed to install new binary ({install_error}); rollback also failed ({rollback_error})"
            )),
        };
    }
    Ok(())
}

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
        replace_with_backup(downloaded, target, &backup, |from, to| {
            std::fs::rename(from, to).map_err(Into::into)
        })
    }
}

/// SHA-256 verification. Returns Ok on either a successful match or "no SHA256SUMS in
/// release" (with a stderr warning so the user sees the degraded trust). Returns Err
/// only on mismatch / missing-entry / fetch failure — all of which must abort.
async fn verify_download(
    release: &Release,
    downloaded: &std::path::Path,
    asset_name: &str,
) -> anyhow::Result<()> {
    let Some(checksums_asset) = find_checksums_asset(release) else {
        eprintln!(
            "warning: this release has no SHA256SUMS file — proceeding with HTTPS trust only. \
             Future releases will include checksums."
        );
        return Ok(());
    };

    let checksums = fetch_checksums(checksums_asset)
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch SHA256SUMS: {e}"))?;

    let expected = lookup_checksum(&checksums, asset_name).ok_or_else(|| {
        anyhow::anyhow!(
            "SHA256SUMS does not contain an entry for {asset_name} — refusing to install"
        )
    })?;

    // Hashing a few-MB file is fast enough that we don't bother with spawn_blocking.
    let actual = sha256_file(downloaded)?;
    if actual.to_ascii_lowercase() != expected {
        anyhow::bail!("SHA-256 mismatch for {asset_name}: expected {expected}, got {actual}");
    }
    println!("Verified SHA-256 ({} = {})", asset_name, &expected[..16]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_with_backup_rolls_back_when_install_rename_fails() {
        let downloaded = std::path::Path::new("downloaded");
        let target = std::path::Path::new("target");
        let backup = std::path::Path::new("backup");
        let mut calls = Vec::new();
        let error = replace_with_backup(downloaded, target, backup, |from, to| {
            calls.push((from.to_owned(), to.to_owned()));
            if calls.len() == 2 { anyhow::bail!("install rename failed") }
            Ok(())
        })
        .unwrap_err();

        assert_eq!(calls, vec![
            (target.to_owned(), backup.to_owned()),
            (downloaded.to_owned(), target.to_owned()),
            (backup.to_owned(), target.to_owned()),
        ]);
        assert!(error.to_string().contains("install rename failed"));
    }

    #[test]
    fn replace_with_backup_reports_install_and_rollback_failures() {
        let mut call = 0;
        let error = replace_with_backup(
            std::path::Path::new("downloaded"),
            std::path::Path::new("target"),
            std::path::Path::new("backup"),
            |_, _| {
                call += 1;
                match call {
                    2 => anyhow::bail!("install rename failed"),
                    3 => anyhow::bail!("rollback failed"),
                    _ => Ok(()),
                }
            },
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("install rename failed"));
        assert!(message.contains("rollback failed"));
    }

    #[test]
    fn lookup_checksum_matches_exact_filename() {
        let body = "\
abc123def456abc123def456abc123def456abc123def456abc123def456abcd  claude-lifeline-aarch64-apple-darwin
111122223333111122223333111122223333111122223333111122223333ffff  claude-lifeline-x86_64-unknown-linux-musl
";
        let got = lookup_checksum(body, "claude-lifeline-aarch64-apple-darwin").unwrap();
        assert_eq!(got, "abc123def456abc123def456abc123def456abc123def456abc123def456abcd");
    }

    #[test]
    fn lookup_checksum_rejects_invalid_digest() {
        // Wrong length — defends against a malformed/truncated SHA256SUMS that could
        // be silently accepted otherwise
        let body = "deadbeef  claude-lifeline-aarch64-apple-darwin\n";
        assert!(lookup_checksum(body, "claude-lifeline-aarch64-apple-darwin").is_none());
    }

    #[test]
    fn lookup_checksum_handles_binary_marker() {
        // GNU sha256sum may emit `* filename` for binary mode
        let body = "\
abc123def456abc123def456abc123def456abc123def456abc123def456abcd *claude-lifeline-x86_64-pc-windows-msvc.exe
";
        assert_eq!(
            lookup_checksum(body, "claude-lifeline-x86_64-pc-windows-msvc.exe"),
            Some("abc123def456abc123def456abc123def456abc123def456abc123def456abcd".into())
        );
    }

    #[test]
    fn lookup_checksum_ignores_comments_and_blank_lines() {
        let body = "\
# generated 2026-05-16

abc123def456abc123def456abc123def456abc123def456abc123def456abcd  claude-lifeline-aarch64-apple-darwin
";
        assert!(lookup_checksum(body, "claude-lifeline-aarch64-apple-darwin").is_some());
    }

    #[test]
    fn lookup_checksum_returns_none_for_missing_entry() {
        let body = "abc123def456abc123def456abc123def456abc123def456abc123def456abcd  other-binary\n";
        assert!(lookup_checksum(body, "claude-lifeline-aarch64-apple-darwin").is_none());
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

        // Verify SHA-256 before doing anything else with the file. A failed verification
        // aborts the upgrade; we leave the temp file in place only long enough to delete
        // it, never copy it into position. Check is best-effort: older releases lack
        // SHA256SUMS — we print a warning and let the user decide whether to continue.
        if let Err(e) = super::verify_download(&release, &temp, &asset.name).await {
            let _ = std::fs::remove_file(&temp);
            return Err(e);
        }

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
