use serde::Deserialize;

// ── ~/.claude/.credentials.json 凭证读取 ──

#[derive(Debug, Deserialize)]
pub struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    pub claude_ai_oauth: Option<OAuthCredential>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCredential {
    #[serde(rename = "accessToken")]
    pub access_token: Option<String>,
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

    if path.exists() {
        let content = std::fs::read_to_string(&path).ok()?;
        let creds: CredentialsFile = serde_json::from_str(&content).ok()?;
        return creds.claude_ai_oauth;
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
