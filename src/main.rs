mod auth;
mod cli;
mod config;
mod data;
mod doctor;
mod git;
mod history;
mod input;
mod json_render;
mod render;
mod tui;
mod update;
mod usage;
mod watch;

use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();

    // 保留旧的隐藏顶层 flag（install/spawn 子进程使用）
    if cli.check_update {
        crate::update::do_update_check().await;
        return;
    }

    let exit = cli::dispatch(cli).await;
    if let Err(e) = exit {
        // 只有非 statusline 路径才打印错误；statusline 内部已自行静默
        eprintln!("claude-lifeline: {e}");
        std::process::exit(1);
    }
}
