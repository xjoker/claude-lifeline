//! Prompt cache 存活倒计时 + 真实过期探测
//!
//! 核心思路：
//! 1. stdin 的 `current_usage` 是"最近一次 API 调用"的快照，cache_read 不为 0 说明那次命中了 cache
//! 2. 每次命中都会刷新服务器端 TTL 到 +5min，所以记录"最近一次观察到 cache_read>0 的时刻"即可推算预计过期时间
//! 3. **真实过期探测（A+C 组合）**：观察到 `cache_read` 从 N>0 跳到 0 的瞬间，结合
//!    `cache_creation` vs `input_tokens` 比例区分两种"cache 死亡"成因：
//!    - `cache_creation >> input_tokens` → 真实 TTL 过期后服务器重建 prefix（标记 just_expired）
//!    - `cache_creation <= input_tokens`  → /compact 或新会话首条（不显示 expired 提示）
//!
//! 不可能 100% 准确（5min 是约定俗成默认，Anthropic 未承诺；TOCTOU 上还有 statusline 调用滞后）。
//! 用户语义：alive=true 表示"cache 大概率还在"，just_expired 表示"刚检测到真实过期，
//!         你这条消息为重建 cache 多花了钱"。

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::input::StdinData;

/// 默认 prompt cache TTL（秒）。Anthropic 默认 5 分钟。
const TTL_SECS: i64 = 5 * 60;

/// "刚过期"提示的可见窗口（秒）。检测到真实过期后这段时间内显示 `expired` 标记
const JUST_EXPIRED_WINDOW_SECS: i64 = 60;

/// 真实过期判定阈值：cache_creation >= input_tokens * EXPIRY_RATIO 视为 prefix 重建
/// /compact 后下一条 input_tokens 通常较大（compact 后的摘要），ratio 偏小
/// TTL 过期后下一条 cache_creation 远大于 input_tokens（整个 prefix 重写）
const EXPIRY_RATIO: u64 = 2;

/// 持久化到 ~/.claude/claude-lifeline/cache-ttl-<session_id>.json
///
/// 每个 session 一个独立文件——多个 CC 终端并行运行时，避免不同 session_id 互相覆盖
/// 同一个文件导致 last_active_at 被反复重置到 now（倒计时永远不下降）
#[derive(Serialize, Deserialize, Default)]
struct CacheTtlFile {
    session_id: String,
    /// 上次观察到 cache_read 时它的具体值——用于检测"是不是同一次 API 调用的重复显示"
    last_cache_read: u64,
    /// 上次观察到 cache_read > 0 的 unix 时间戳（秒）
    last_active_at: i64,
    /// 真实 TTL 过期检测时刻（cache_read 从 >0 跳到 0 且 cache_creation 占比高）
    /// `None` 表示当前未处于"刚过期"提示窗口
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_expired_at: Option<i64>,
}

/// 渲染需要的状态。
#[derive(Debug, Clone, Copy)]
pub struct CacheLiveState {
    pub alive: bool,
    pub remaining_secs: i64,
    /// 距离最近一次真实过期事件的秒数。Some(s) 且 s ≤ JUST_EXPIRED_WINDOW_SECS 时
    /// 渲染层显示 `expired` 提示
    pub just_expired_secs: Option<i64>,
    /// 本次观察到的 cache 命中率（百分比 0..=100）。None 表示总 token 为 0
    /// 不可分母。集中在此处暴露，避免 render 层重复读取 stdin
    pub hit_percent: Option<f64>,
}

/// 检查并更新 cache 存活状态。每次 statusline 启动时调用一次。
///
/// 返回 None 表示 stdin 缺少必要字段（current_usage=null 等），调用方不渲染倒计时。
pub fn check_and_update(stdin: &StdinData) -> Option<CacheLiveState> {
    let session_id = stdin.session_id.as_deref()?.to_string();
    let usage = stdin.context_window.as_ref()?.current_usage.as_ref()?;
    let cache_read = usage.cache_read_input_tokens.unwrap_or(0);
    let cache_creation = usage.cache_creation_input_tokens.unwrap_or(0);
    let input = usage.input_tokens.unwrap_or(0);
    let total = input + cache_read + cache_creation;
    let hit_percent = if total > 0 {
        Some((cache_read as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
    } else {
        None
    };
    let now = chrono::Utc::now().timestamp();

    let prev = read_state(&session_id);

    if cache_read == 0 {
        // cache 死亡观察。区分四种来源：
        //   (a) prev.last_cache_read > 0 + 高 cache_creation 比例 → 真实 TTL 过期，标记
        //   (b) prev.last_cache_read > 0 + 低 cache_creation 比例 → /compact 等，清状态
        //   (c) prev.last_expired_at 已有 → 沿用，渲染层判窗口
        //   (d) 无 prev → 新会话首条或长期 dead，无信号
        let just_expired_secs = match &prev {
            Some(p) if p.last_cache_read > 0 => {
                let is_real_expiry = cache_creation >= input.max(1) * EXPIRY_RATIO;
                let elapsed = now - p.last_active_at;
                if is_real_expiry {
                    // 转入"刚过期"窗口：保留 last_active_at 作为审计，新增 last_expired_at = now
                    write_state(&session_id, &CacheTtlFile {
                        session_id: session_id.clone(),
                        last_cache_read: 0,
                        last_active_at: p.last_active_at,
                        last_expired_at: Some(now),
                    });
                    // Phase 1：采集实测 TTL 样本（不影响预测，只记录）
                    crate::ttl_samples::record(
                        elapsed,
                        cache_creation,
                        input,
                        p.last_cache_read,
                    );
                    log_decision(
                        &session_id,
                        "real_expiry",
                        p.last_cache_read,
                        cache_read,
                        cache_creation,
                        input,
                        Some(elapsed),
                    );
                    Some(0)
                } else {
                    // /compact 或类似——静默清状态
                    clear_state(&session_id);
                    log_decision(
                        &session_id,
                        "compact_or_first",
                        p.last_cache_read,
                        cache_read,
                        cache_creation,
                        input,
                        Some(elapsed),
                    );
                    None
                }
            }
            Some(p) => p.last_expired_at.map(|t| now - t),
            None => None,
        };

        return Some(CacheLiveState {
            alive: false,
            remaining_secs: 0,
            just_expired_secs,
            hit_percent,
        });
    }

    // cache_read > 0：cache 活着
    let same_call = prev
        .as_ref()
        .map(|p| p.last_cache_read == cache_read)
        .unwrap_or(false);
    let last_active_at = if same_call {
        // 同一次 API 调用的重复观察——保留时间戳，但若有 last_expired_at 也清掉（不应同时有效）
        prev.as_ref().map(|p| p.last_active_at).unwrap_or(now)
    } else {
        // 新 API 调用，刷新时间戳，清除"刚过期"标记
        let elapsed = prev.as_ref().map(|p| now - p.last_active_at);
        write_state(&session_id, &CacheTtlFile {
            session_id: session_id.clone(),
            last_cache_read: cache_read,
            last_active_at: now,
            last_expired_at: None,
        });
        log_decision(
            &session_id,
            "new_call",
            prev.as_ref().map(|p| p.last_cache_read).unwrap_or(0),
            cache_read,
            cache_creation,
            input,
            elapsed,
        );
        now
    };

    let remaining = (last_active_at + TTL_SECS) - now;
    Some(CacheLiveState {
        alive: remaining > 0,
        remaining_secs: remaining.max(0),
        just_expired_secs: None,
        hit_percent,
    })
}

/// 是否还在"刚过期"提示窗口内
pub fn within_expired_window(state: &CacheLiveState) -> bool {
    state.just_expired_secs
        .map(|s| s >= 0 && s <= JUST_EXPIRED_WINDOW_SECS)
        .unwrap_or(false)
}

/// 格式化倒计时：`Xm Ys` / `Xm` / `Ys`。控制在 ≤ 5 列以内。
pub fn format_remaining(secs: i64) -> String {
    if secs <= 0 {
        return "0s".to_string();
    }
    let m = secs / 60;
    let s = secs % 60;
    if m == 0 {
        format!("{s}s")
    } else if s == 0 {
        format!("{m}m")
    } else {
        format!("{m}m{s}s")
    }
}

/// session_id 走文件名——只允许 [A-Za-z0-9._-]，其他全替换 _ 防止路径注入
fn sanitize_session_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .collect()
}

fn state_path(session_id: &str) -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join("claude-lifeline")
        .join(format!("cache-ttl-{}.json", sanitize_session_id(session_id)))
}

fn read_state(session_id: &str) -> Option<CacheTtlFile> {
    let content = std::fs::read_to_string(state_path(session_id)).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_state(session_id: &str, state: &CacheTtlFile) {
    let path = state_path(session_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(state) {
        let _ = std::fs::write(path, json);
    }
}

fn clear_state(session_id: &str) {
    let _ = std::fs::remove_file(state_path(session_id));
}

// ── 诊断日志 ──
//
// 每次"分类决策"事件追加一行到 ~/.claude/claude-lifeline/cache-decisions.jsonl
// 作用：让"为什么没采到样本"可审计——区分真没过期 / 误分类成 compact。
// 失败静默，不影响主流程。
//
// category 取值：
//   real_expiry      — cache_read 0 且 cache_creation >= input * 2
//   compact_or_first — cache_read 0 但 cache_creation 占比低
//   new_call         — cache_read > 0 且与上次值不同（新 API 调用）

const DECISIONS_MAX_BYTES: u64 = 200_000;
const DECISIONS_MAX_LINES: usize = 1000;
const DECISIONS_MAX_AGE_SECS: i64 = 90 * 24 * 3600;

#[derive(Serialize)]
struct DecisionRecord<'a> {
    ts: i64,
    session: &'a str,
    category: &'a str,
    prev_cache_read: u64,
    cache_read: u64,
    cache_creation: u64,
    input: u64,
    ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_since_prev_secs: Option<i64>,
}

fn log_decision(
    session_id: &str,
    category: &str,
    prev_cache_read: u64,
    cache_read: u64,
    cache_creation: u64,
    input: u64,
    elapsed_since_prev_secs: Option<i64>,
) {
    let rec = DecisionRecord {
        ts: chrono::Utc::now().timestamp(),
        session: session_id,
        category,
        prev_cache_read,
        cache_read,
        cache_creation,
        input,
        ratio: if input > 0 {
            cache_creation as f64 / input as f64
        } else {
            f64::INFINITY
        },
        elapsed_since_prev_secs,
    };
    let Ok(json) = serde_json::to_string(&rec) else { return };
    let path = decisions_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else { return };
    let _ = writeln!(f, "{json}");
    drop(f);

    maybe_compact_decisions(&path);
}

fn decisions_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join("claude-lifeline")
        .join("cache-decisions.jsonl")
}

fn maybe_compact_decisions(path: &PathBuf) {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size < DECISIONS_MAX_BYTES {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else { return };
    let cutoff = chrono::Utc::now().timestamp() - DECISIONS_MAX_AGE_SECS;

    let mut kept: Vec<&str> = content
        .lines()
        .filter(|line| {
            #[derive(Deserialize)]
            struct TsOnly { ts: i64 }
            serde_json::from_str::<TsOnly>(line)
                .map(|t| t.ts >= cutoff)
                .unwrap_or(false)
        })
        .collect();

    if kept.len() > DECISIONS_MAX_LINES {
        let drop_n = kept.len() - DECISIONS_MAX_LINES;
        kept.drain(..drop_n);
    }

    let mut new_content = kept.join("\n");
    if !new_content.is_empty() {
        new_content.push('\n');
    }
    let tmp = path.with_extension("jsonl.tmp");
    if std::fs::write(&tmp, &new_content).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}
