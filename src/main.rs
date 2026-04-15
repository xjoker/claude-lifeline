mod auth;
mod git;
mod input;
mod render;
mod usage;

#[tokio::main]
async fn main() {
    if let Err(_) = run().await {
        return;
    }
}

async fn run() -> anyhow::Result<()> {
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

    // 3. 会话时长：从 transcript 文件创建时间推算
    let session_duration = stdin.transcript_path.as_deref()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.created().ok())
        .and_then(|t| t.elapsed().ok());

    // 4. 并发：git info + usage data
    let (git, usage) = tokio::join!(
        crate::git::get_git_info(&cwd),
        crate::usage::get_usage_data(stdin.rate_limits.as_ref()),
    );

    // 5. 渲染输出
    let ctx = crate::render::RenderContext { stdin, git, usage, session_duration };
    crate::render::render(&ctx);

    Ok(())
}
