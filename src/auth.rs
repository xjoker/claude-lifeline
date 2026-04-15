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
    #[serde(rename = "subscriptionType")]
    pub subscription_type: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<u64>,
}

/// 计划名称（从 subscriptionType 映射）
#[derive(Debug, Clone, PartialEq)]
pub enum PlanName {
    Max,
    Pro,
    Team,
    Unknown(String),
}

impl std::fmt::Display for PlanName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanName::Max => write!(f, "Max"),
            PlanName::Pro => write!(f, "Pro"),
            PlanName::Team => write!(f, "Team"),
            PlanName::Unknown(s) => write!(f, "{s}"),
        }
    }
}

// ── 公共函数 ──

/// 读取凭证文件，返回 OAuth 信息。macOS 上若文件不存在则静默返回 None
pub fn read_credentials() -> Option<OAuthCredential> {
    let home = std::env::var("HOME").ok()?;
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

/// 从 subscriptionType 映射到 PlanName
pub fn parse_plan_name(subscription_type: &str) -> PlanName {
    let lower = subscription_type.to_lowercase();
    if lower.contains("max") {
        PlanName::Max
    } else if lower.contains("pro") {
        PlanName::Pro
    } else if lower.contains("team") {
        PlanName::Team
    } else {
        PlanName::Unknown(subscription_type.to_string())
    }
}
