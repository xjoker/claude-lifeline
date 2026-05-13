mod auth;
mod cache_ttl;
mod config;
mod git;
mod input;
mod render;
mod ttl_samples;
mod update;
mod usage;

#[tokio::main]
async fn main() {
    if run().await.is_err() {
        // 任何失败都静默退出（statusline 不该污染终端），诊断信息由模块各自写 stderr
    }
}

async fn run() -> anyhow::Result<()> {
    // --version 支持（安装脚本版本检查用）
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // --check-update：后台子进程执行网络检查
    if args.iter().any(|a| a == "--check-update") {
        crate::update::do_update_check().await;
        return Ok(());
    }

    // 1. 读 stdin JSON
    let stdin = crate::input::read_stdin().await?;

    // 2. 获取 cwd 用于 git
    let cwd = stdin
        .cwd
        .clone()
        .or_else(|| {
            stdin
                .workspace
                .as_ref()
                .and_then(|w| w.current_dir.clone())
        })
        .unwrap_or_default();

    // 3. 并发：git info + usage data
    let (git, usage) = tokio::join!(
        crate::git::get_git_info(&cwd),
        crate::usage::get_usage_data(stdin.rate_limits.as_ref()),
    );

    // 4. 读取配置
    let config = crate::config::read_config();

    // 5. 升级提示（纯文件读取，sub-ms）
    let update_hint = crate::update::check_update_hint();
    crate::update::ensure_cache_exists();

    // 6. 渲染输出
    let ctx = crate::render::RenderContext { stdin, git, usage, config, update_hint };
    crate::render::render(&ctx);

    Ok(())
}
