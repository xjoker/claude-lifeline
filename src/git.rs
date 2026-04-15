// ── git 状态获取（异步，500ms 超时） ──

#[derive(Debug, Default)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub is_dirty: bool,
}

/// 异步获取 git branch + dirty 状态，500ms 超时后返回默认值
pub async fn get_git_info(cwd: &str) -> GitInfo {
    todo!()
}
