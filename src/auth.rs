use serde::Deserialize;
use std::fmt;

// ── ~/.claude/.credentials.json 凭证读取 ──

#[derive(Deserialize)]
pub struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    pub claude_ai_oauth: Option<OAuthCredential>,
}

#[derive(Deserialize)]
pub struct OAuthCredential {
    #[serde(rename = "accessToken")]
    pub access_token: Option<String>,
}

// 自定义 Debug 遮蔽 access_token —— 防止意外通过 `{:?}` / panic / anyhow context
// 把 Bearer token 泄漏到 stderr / 系统日志
impl fmt::Debug for OAuthCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthCredential")
            .field("access_token", &self.access_token.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl fmt::Debug for CredentialsFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialsFile")
            .field("claude_ai_oauth", &self.claude_ai_oauth)
            .finish()
    }
}

// ── 公共函数 ──

/// 读取凭证：优先 ~/.claude/.credentials.json，macOS 上回退到 Keychain
pub fn read_credentials() -> Option<OAuthCredential> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".claude")
        .join(".credentials.json");

    // 限制 64 KiB：防 symlink-to-/dev/zero 等吞内存攻击；
    // 真实凭证文件实测 <2 KiB，64 KiB 足够裕度
    if let Some(content) = read_capped(&path, 64 * 1024) {
        if let Ok(creds) = serde_json::from_str::<CredentialsFile>(&content) {
            return creds.claude_ai_oauth;
        }
    }

    // macOS Keychain 回退（service = "Claude Code-credentials"）
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("security")
            .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
            .output()
        {
            Ok(output) if output.status.success() => {
                match String::from_utf8(output.stdout) {
                    Ok(json) => match serde_json::from_str::<CredentialsFile>(json.trim()) {
                        Ok(creds) => return creds.claude_ai_oauth,
                        Err(e) => eprintln!("claude-lifeline: parse Keychain creds failed: {e}"),
                    },
                    Err(e) => eprintln!("claude-lifeline: Keychain output not UTF-8: {e}"),
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "claude-lifeline: security exited {} — {}",
                    output.status,
                    stderr.trim()
                );
            }
            Err(e) => eprintln!("claude-lifeline: spawn security failed: {e}"),
        }
    }

    None
}

/// 同步读文件，限制最大字节数。超过上限返回 None
fn read_capped(path: &std::path::Path, max_bytes: u64) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = String::new();
    f.by_ref().take(max_bytes).read_to_string(&mut buf).ok()?;
    Some(buf)
}
