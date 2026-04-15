// ── git 状态获取（异步，500ms 超时） ──

use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Default)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub is_dirty: bool,
}

/// 异步获取 git branch + dirty 状态，500ms 超时后返回默认值
pub async fn get_git_info(cwd: &str) -> GitInfo {
    let deadline = Duration::from_millis(500);

    let branch_fut = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    let dirty_fut = Command::new("git")
        .args(["--no-optional-locks", "status", "--porcelain"])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    let (branch_res, dirty_res) = tokio::join!(
        timeout(deadline, branch_fut),
        timeout(deadline, dirty_fut),
    );

    let branch = branch_res
        .ok()
        .and_then(|r| r.ok())
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        });

    let is_dirty = dirty_res
        .ok()
        .and_then(|r| r.ok())
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    GitInfo { branch, is_dirty }
}
