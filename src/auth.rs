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
    /// 订阅类型：观测到的值 "max" / "pro" / "free"；大小写以 Anthropic 返回为准
    #[serde(rename = "subscriptionType", default)]
    pub subscription_type: Option<String>,
    /// 速率档：观测到的值如 "default_claude_max_20x" / "default_claude_max_5x" /
    /// "default_claude_pro"；比 subscription_type 更精细，可区分 Max 20x vs 5x
    #[serde(rename = "rateLimitTier", default)]
    pub rate_limit_tier: Option<String>,
}

// 自定义 Debug 遮蔽 access_token —— 防止意外通过 `{:?}` / panic / anyhow context
// 把 Bearer token 泄漏到 stderr / 系统日志
impl fmt::Debug for OAuthCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthCredential")
            .field("access_token", &self.access_token.as_ref().map(|_| "[REDACTED]"))
            .field("subscription_type", &self.subscription_type)
            .field("rate_limit_tier", &self.rate_limit_tier)
            .finish()
    }
}

/// 把 (subscription_type, rate_limit_tier) 解析为状态栏紧凑标签。
///
/// 优先级：rate_limit_tier > subscription_type。两者都缺返回 None。
///
/// 已观测到的速率档命名规律（Anthropic 公开 OAuth 字段，2026-05）：
///   `default_claude_<plan>` 或 `default_claude_<plan>_<multiplier>`
/// 解析时取末段，识别 `max_20x`/`max_5x`/`pro` 等；未知值大写降级处理。
pub fn subscription_label(
    subscription_type: Option<&str>,
    rate_limit_tier: Option<&str>,
) -> Option<String> {
    if let Some(tier) = rate_limit_tier.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(label) = parse_rate_limit_tier(tier) {
            return Some(label);
        }
    }
    subscription_type
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
}

fn parse_rate_limit_tier(tier: &str) -> Option<String> {
    let suffix = tier.strip_prefix("default_claude_").unwrap_or(tier);
    let mut parts = suffix.split('_');
    let plan = parts.next()?;
    let multiplier = parts.next();
    let label = match (plan, multiplier) {
        ("max", Some(m)) => format!("MAX·{m}"),
        ("max", None) => "MAX".to_string(),
        ("pro", _) => "PRO".to_string(),
        ("free", _) => "FREE".to_string(),
        ("team", _) => "TEAM".to_string(),
        ("enterprise", _) => "ENT".to_string(),
        // 未识别 plan：保留原始 tier 末段大写，避免吞掉新枚举值
        (other, Some(m)) => format!("{}·{}", other.to_ascii_uppercase(), m),
        (other, None) => other.to_ascii_uppercase(),
    };
    Some(label)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_access_token() {
        let cred = OAuthCredential {
            access_token: Some("sk-ant-oat-supersecret".into()),
            subscription_type: Some("max".into()),
            rate_limit_tier: Some("default_claude_max_20x".into()),
        };
        let dump = format!("{cred:?}");
        assert!(!dump.contains("supersecret"), "token leaked: {dump}");
        assert!(dump.contains("[REDACTED]"));
        // 非敏感字段保持可见
        assert!(dump.contains("max"));
        assert!(dump.contains("default_claude_max_20x"));
    }

    #[test]
    fn deserializes_full_keychain_payload() {
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "tok",
                "refreshToken": "rtok",
                "expiresAt": 1778962420854,
                "subscriptionType": "max",
                "rateLimitTier": "default_claude_max_20x"
            }
        }"#;
        let creds: CredentialsFile = serde_json::from_str(json).unwrap();
        let oauth = creds.claude_ai_oauth.unwrap();
        assert_eq!(oauth.access_token.as_deref(), Some("tok"));
        assert_eq!(oauth.subscription_type.as_deref(), Some("max"));
        assert_eq!(
            oauth.rate_limit_tier.as_deref(),
            Some("default_claude_max_20x")
        );
    }

    #[test]
    fn legacy_credentials_without_new_fields() {
        // 老版本凭证（subscriptionType/rateLimitTier 未写入时）
        let json = r#"{"claudeAiOauth": {"accessToken": "tok"}}"#;
        let creds: CredentialsFile = serde_json::from_str(json).unwrap();
        let oauth = creds.claude_ai_oauth.unwrap();
        assert!(oauth.subscription_type.is_none());
        assert!(oauth.rate_limit_tier.is_none());
    }

    #[test]
    fn label_prefers_rate_limit_tier() {
        assert_eq!(
            subscription_label(Some("max"), Some("default_claude_max_20x")),
            Some("MAX·20x".into())
        );
        assert_eq!(
            subscription_label(Some("max"), Some("default_claude_max_5x")),
            Some("MAX·5x".into())
        );
        assert_eq!(
            subscription_label(Some("pro"), Some("default_claude_pro")),
            Some("PRO".into())
        );
    }

    #[test]
    fn label_falls_back_to_subscription_type_when_tier_missing() {
        assert_eq!(
            subscription_label(Some("free"), None),
            Some("FREE".into())
        );
        assert_eq!(subscription_label(Some("max"), Some("")), Some("MAX".into()));
    }

    #[test]
    fn label_returns_none_when_both_empty() {
        assert_eq!(subscription_label(None, None), None);
        assert_eq!(subscription_label(Some(""), Some("  ")), None);
    }

    #[test]
    fn label_handles_unknown_tier_gracefully() {
        // 新出的 plan 名（如 team / enterprise / 未知）不丢，保留大写
        assert_eq!(
            subscription_label(None, Some("default_claude_team")),
            Some("TEAM".into())
        );
        assert_eq!(
            subscription_label(None, Some("default_claude_enterprise")),
            Some("ENT".into())
        );
        // 完全未知的 plan + multiplier 组合
        assert_eq!(
            subscription_label(None, Some("default_claude_galaxy_99x")),
            Some("GALAXY·99x".into())
        );
        // 没有 default_claude_ 前缀也应工作
        assert_eq!(
            subscription_label(None, Some("legacy_max_2x")),
            Some("LEGACY·max".into())
        );
    }
}
