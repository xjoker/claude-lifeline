use serde::Deserialize;

// ── stdin JSON 结构体（Claude Code 每次 spawn 时通过 stdin 推送） ──

#[derive(Debug, Default, Deserialize)]
pub struct StdinData {
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<ModelInfo>,
    pub workspace: Option<WorkspaceInfo>,
    pub version: Option<String>,
    pub cost: Option<CostInfo>,
    pub context_window: Option<ContextWindow>,
    pub rate_limits: Option<RateLimits>,
}

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceInfo {
    pub current_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CostInfo {
    pub total_cost_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ContextWindow {
    pub used_percentage: Option<f64>,
    pub context_window_size: Option<u64>,
    pub current_usage: Option<TokenUsage>,
}

#[derive(Debug, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimits {
    pub five_hour: Option<RateLimitWindow>,
    pub seven_day: Option<RateLimitWindow>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimitWindow {
    pub used_percentage: Option<f64>,
    pub resets_at: Option<String>,
}

// ── 公共函数 ──

/// 从 stdin 读取并解析 JSON
pub async fn read_stdin() -> anyhow::Result<StdinData> {
    use tokio::io::AsyncReadExt;

    let mut buf = String::new();
    tokio::io::stdin().read_to_string(&mut buf).await?;
    let data: StdinData = serde_json::from_str(&buf)?;
    Ok(data)
}

/// 获取模型显示名称
pub fn get_model_name(stdin: &StdinData) -> String {
    if let Some(model) = &stdin.model {
        if let Some(name) = &model.display_name {
            if !name.is_empty() {
                return name.clone();
            }
        }
        if let Some(id) = &model.id {
            return id.clone();
        }
    }
    "Unknown".to_string()
}

/// 获取 context 使用百分比（优先 native，回退手动计算）
pub fn get_context_percent(stdin: &StdinData) -> f64 {
    if let Some(ctx) = &stdin.context_window {
        // 优先使用 used_percentage
        if let Some(pct) = ctx.used_percentage {
            return pct.clamp(0.0, 100.0);
        }

        // 回退：手动计算
        if let (Some(usage), Some(window_size)) = (&ctx.current_usage, ctx.context_window_size) {
            if window_size > 0 {
                let input = usage.input_tokens.unwrap_or(0) as f64;
                let cache_create = usage.cache_creation_input_tokens.unwrap_or(0) as f64;
                let cache_read = usage.cache_read_input_tokens.unwrap_or(0) as f64;
                let total = input + cache_create + cache_read;
                let pct = (total / window_size as f64) * 100.0;
                return pct.clamp(0.0, 100.0);
            }
        }
    }
    0.0
}
