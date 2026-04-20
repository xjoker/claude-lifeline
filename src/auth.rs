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

/// 读取凭证文件，返回 OAuth 信息。macOS 上若文件不存在则静默返回 None
pub fn read_credentials() -> Option<OAuthCredential> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".claude")
        .join(".credentials.json");

    // macOS: 文件不存在时静默返回 None（不访问 Keychain）
    if cfg!(target_os = "macos") && !path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    let creds: CredentialsFile = serde_json::from_str(&content).ok()?;
    creds.claude_ai_oauth
}
