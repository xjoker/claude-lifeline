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

/// 订阅标签的最大显示宽度（字符数；ASCII 字符按 1 列估算）。
/// 防御外部字段被异常拉长撑爆 subscription 块；常见标签 `MAX·20x` 才 7 字符
const SUBSCRIPTION_LABEL_MAX_CHARS: usize = 16;

/// 把 (subscription_type, rate_limit_tier) 解析为状态栏紧凑标签。
///
/// 优先级：rate_limit_tier > subscription_type。两者都缺返回 None。
///
/// 已观测到的速率档命名规律（Anthropic 公开 OAuth 字段，2026-05）：
///   `default_claude_<plan>` 或 `default_claude_<plan>_<multiplier>` 或更长
/// 解析时识别已知 plan（max/pro/free/team/enterprise），用 `·` 拼接剩余段；
/// 未知 plan 大写降级、保留所有段。所有出口统一截断至 `SUBSCRIPTION_LABEL_MAX_CHARS`
/// 防御 API 异常返回（含控制字符的字符也由 `to_ascii_uppercase` 自然保留位置但不破坏
/// ANSI 输出 —— 渲染层进一步过滤）
pub fn subscription_label(
    subscription_type: Option<&str>,
    rate_limit_tier: Option<&str>,
) -> Option<String> {
    let raw = if let Some(tier) = rate_limit_tier.map(str::trim).filter(|s| !s.is_empty()) {
        parse_rate_limit_tier(tier).or_else(|| {
            subscription_type
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_uppercase())
        })?
    } else {
        subscription_type
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_uppercase())?
    };
    Some(truncate_label(&raw, SUBSCRIPTION_LABEL_MAX_CHARS))
}

fn parse_rate_limit_tier(tier: &str) -> Option<String> {
    let suffix = tier.strip_prefix("default_claude_").unwrap_or(tier);
    let parts: Vec<&str> = suffix.split('_').filter(|s| !s.is_empty()).collect();
    let (plan, rest) = parts.split_first()?;
    let label = match *plan {
        "max" if rest.is_empty() => "MAX".to_string(),
        "max" => format!("MAX·{}", rest.join("·")),
        "pro" => "PRO".to_string(),
        "free" => "FREE".to_string(),
        "team" => "TEAM".to_string(),
        "enterprise" => "ENT".to_string(),
        // 未识别 plan：保留所有段，第一段大写做"plan name"提示，其它段保留原 case
        // 避免吞掉未来 Anthropic 引入的 multi-segment 命名（如 `enterprise_premium_50x`）
        _ if rest.is_empty() => plan.to_ascii_uppercase(),
        _ => format!("{}·{}", plan.to_ascii_uppercase(), rest.join("·")),
    };
    Some(label)
}

/// 按字符数截断（非字节）。用于 ASCII / 单字节 label；如果未来引入 CJK label
/// 需要切换到视觉宽度截断（见 render::truncate_visual）
fn truncate_label(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    // 留 2 列给 `..` 提示
    let keep = max_chars.saturating_sub(2);
    let mut out: String = s.chars().take(keep).collect();
    out.push_str("..");
    out
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
        // 没有 default_claude_ 前缀也应工作，且所有段保留（修复原 LEGACY·max 丢段问题）
        assert_eq!(
            subscription_label(None, Some("legacy_max_2x")),
            Some("LEGACY·max·2x".into())
        );
    }

    #[test]
    fn label_preserves_multi_segment_known_plan() {
        // 已知 plan + 多段后缀（防御未来命名扩展），所有段保留，不再丢尾段
        assert_eq!(
            subscription_label(None, Some("default_claude_max_5x_beta")),
            Some("MAX·5x·beta".into())
        );
        // 单 plan 无后缀的几种形式都应稳定输出
        assert_eq!(
            subscription_label(None, Some("default_claude_max")),
            Some("MAX".into())
        );
        // 超过 16 字符的合法 label 由长度守卫截断 + `..` 收尾 —— 验证
        // 多段保留与长度守卫两个机制叠加后的行为
        let long_known = subscription_label(None, Some("default_claude_max_enterprise_20x"))
            .expect("label parses");
        assert!(long_known.starts_with("MAX·enterprise"));
        assert!(long_known.ends_with(".."));
        assert!(long_known.chars().count() <= 16);
    }

    #[test]
    fn label_is_length_bounded() {
        // 模拟 API 返回异常长的 tier 字符串：截断到 16 字符并加 ".."
        let long_tier = format!("default_claude_max_{}", "x".repeat(200));
        let label = subscription_label(None, Some(&long_tier)).unwrap();
        assert!(label.chars().count() <= 16, "label too long: {label}");
        assert!(label.ends_with(".."));
        // subscription_type fallback 也应受限
        let long_sub = "x".repeat(200);
        let label = subscription_label(Some(&long_sub), None).unwrap();
        assert!(label.chars().count() <= 16);
    }
}
