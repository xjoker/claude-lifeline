//! TTL 实测样本采集（Phase 1：只采集，不影响预测）
//!
//! 每次 cache_ttl 检测到"真实 TTL 过期"事件时，把观察到的 cache 实际寿命
//! 记一条到 `~/.claude/claude-lifeline/ttl-samples.jsonl`。
//! Phase 2 会读这些样本来校准 TTL 预测；Phase 1 仅累积数据。
//!
//! ## 写入安全
//! - JSONL 每行 < 4KB，POSIX `O_APPEND` 写入原子，多 session 并发追加不会撕裂
//! - 不调用 fsync——丢一两条样本不影响统计
//!
//! ## 容量控制
//! - 每次写入后检查文件大小，超过 `COMPACT_THRESHOLD_BYTES` 触发压缩
//! - 压缩时：丢弃 >90 天老样本 + 仅保留最近 `MAX_SAMPLES` 条
//! - 压缩用 tempfile + atomic rename，并发压缩最多丢 1-2 条样本，文件本身始终合法

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// 最多保留多少条样本（足够统计稳定）
const MAX_SAMPLES: usize = 200;
/// 样本最大保留天数（避免 Anthropic 行为变化时旧数据拖累）
const MAX_AGE_SECS: i64 = 90 * 24 * 3600;
/// 文件大小超过此阈值就触发压缩。每行约 130 bytes，50KB ≈ 380 行，足以触发压缩到 200 条
const COMPACT_THRESHOLD_BYTES: u64 = 50_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct Sample {
    /// 采样时刻（unix 秒）
    pub ts: i64,
    /// 实测 cache 寿命（秒）= 检测到死亡时刻 - 上次活着时刻
    pub observed_ttl_secs: i64,
    /// 死亡那次观察到的 cache_creation_input_tokens
    pub cache_creation: u64,
    /// 死亡那次观察到的 input_tokens
    pub input: u64,
    /// 死亡前最后一次记录的 cache_read_input_tokens（用于事后判定误判）
    pub prev_cache_read: u64,
}

/// 记录一条样本。失败静默——采集不能影响主流程。
pub fn record(observed_ttl_secs: i64, cache_creation: u64, input: u64, prev_cache_read: u64) {
    let sample = Sample {
        ts: chrono::Utc::now().timestamp(),
        observed_ttl_secs,
        cache_creation,
        input,
        prev_cache_read,
    };
    let Ok(json) = serde_json::to_string(&sample) else { return };
    let path = samples_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else { return };
    let _ = writeln!(f, "{json}");
    drop(f);

    maybe_compact(&path);
}

/// 读取所有样本（Phase 2 用；Phase 1 暂未调用）
#[allow(dead_code)]
pub fn read_all() -> Vec<Sample> {
    let Ok(content) = std::fs::read_to_string(samples_path()) else { return Vec::new() };
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn samples_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join("claude-lifeline")
        .join("ttl-samples.jsonl")
}

/// 文件超过阈值就压缩。多 session 并发触发时 atomic rename 保证文件始终合法
fn maybe_compact(path: &PathBuf) {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size < COMPACT_THRESHOLD_BYTES {
        return;
    }
    compact(path);
}

fn compact(path: &PathBuf) {
    let Ok(content) = std::fs::read_to_string(path) else { return };
    let cutoff = chrono::Utc::now().timestamp() - MAX_AGE_SECS;

    // 解析 + 过滤 + 截尾，保持原始 JSON 字符串，避免重新 serialize
    let mut kept: Vec<&str> = content
        .lines()
        .filter(|line| {
            // 仅 deserialize 出 ts 字段做时效过滤；坏行（解析失败）一并丢弃
            #[derive(Deserialize)]
            struct TsOnly { ts: i64 }
            serde_json::from_str::<TsOnly>(line)
                .map(|t| t.ts >= cutoff)
                .unwrap_or(false)
        })
        .collect();

    if kept.len() > MAX_SAMPLES {
        let drop_n = kept.len() - MAX_SAMPLES;
        kept.drain(..drop_n);
    }

    let mut new_content = kept.join("\n");
    if !new_content.is_empty() {
        new_content.push('\n');
    }

    // tempfile + atomic rename：并发压缩最多互相覆盖，文件本身永远合法
    let tmp = path.with_extension("jsonl.tmp");
    if std::fs::write(&tmp, &new_content).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}
