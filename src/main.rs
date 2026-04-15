mod auth;
mod git;
mod input;
mod render;
mod usage;

#[tokio::main]
async fn main() {
    // 1. 读 stdin JSON
    // 2. 并发：git info + usage data
    // 3. 渲染输出
    // 任何错误静默退出（status line 不能 panic）
    todo!()
}
