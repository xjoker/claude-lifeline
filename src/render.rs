use crate::git::GitInfo;
use crate::input::StdinData;
use crate::usage::UsageData;

// ── ANSI 颜色常量 ──

pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD_WHITE: &str = "\x1b[1;37m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const BRIGHT_BLUE: &str = "\x1b[94m";
pub const BRIGHT_MAGENTA: &str = "\x1b[95m";

// ── 渲染上下文 ──

pub struct RenderContext {
    pub stdin: StdinData,
    pub git: GitInfo,
    pub usage: UsageData,
}

// ── 公共函数 ──

/// 渲染两行状态栏，输出到 stdout
///
/// 行1: [Sonnet 4.6 | Max]  claude-lifeline  git:(main*)
/// 行2: ctx ████████░░ 45%  │  5h ████|██░░ 65%(1h 45m ↑)  │  7d ██|░░░░░░ 22%(4d 3h ↓)
pub fn render(ctx: &RenderContext) {
    todo!()
}
