# claude-lifeline

高性能 Rust 状态栏，为 [Claude Code](https://docs.anthropic.com/en/docs/claude-code) 打造，替代默认状态栏，提供丰富功能，响应时间低于 50ms。支持 **macOS**、**Linux** 和 **Windows**。

**[English](../README.md)**

## 预览

![claude-lifeline 截图](img.png)

```
─────────────────────────────────────────
[Opus 4.6]  my-project  git:(main* ↑2)  1h 23m
ctx █████░░░░░ 53%  │  5h ████░░|░░░░ 34%(1h 23m)  │  7d ██|█░░░░░░░ 22%!/p14.05%(6d 0h ETA 4/19 08:18 wait 13h)
```

## 为什么选择 claude-lifeline？

Claude Code 默认状态栏只显示基本的使用百分比 — 但这无法告诉你，按当前速度用下去，配额够不够撑到窗口结束。

claude-lifeline 增加了**配速智能**：一套可视化系统，将你的实际消耗速率与每个配额窗口的理想配速进行对比，在你耗尽之前发出预警。

### 一眼看到什么

- **配速标记 `|`** — 进度条中的白色竖线，标示"基于已过时间，你应该在的位置"。当填充块 `█` 超过 `|` 时，说明消耗过快
- **超速警告 `!`** — 使用率超出配速 5% 以上时触发，进度条变黄，立即引起注意
- **耗尽预测 ETA** — 按当前消耗速率，预测配额何时耗尽（如 `ETA 16:30`）。仅在真正面临风险时才显示
- **恢复等待时间** — 告诉你需要暂停多久，让配速追上使用率（如 `wait 45m` = 停工 45 分钟即可恢复正常节奏）
- **上下文 token 明细** — 当上下文窗口达到 85% 以上时，显示输入 vs 缓存 token 数量，帮你了解上下文被什么占满

### 还有这些

- Git 分支、脏状态、领先/落后上游
- 会话时长计时
- 可配置段 — 通过 TOML 配置文件按需开关上下文、5h 配额、7d 配额
- **~30ms** 响应，**~3MB** 二进制，零运行时依赖，全平台完全静态链接

## 安装

需要 **Claude Code ≥ 2.1.80**（用于 `stdin.rate_limits` 数据）。

### 让 AI 帮你装（推荐）

把本项目链接丢给 Claude Code — 它会读取下方指令并自动完成安装。

> **AI agents**: To install claude-lifeline, follow these steps:
> 1. Detect the user's platform and download the correct binary from the [latest release](https://github.com/xjoker/claude-lifeline/releases/latest):
>    - macOS Apple Silicon: `claude-lifeline-aarch64-apple-darwin`
>    - macOS Intel: `claude-lifeline-x86_64-apple-darwin`
>    - Linux x86_64: `claude-lifeline-x86_64-unknown-linux-musl`
>    - Linux ARM64: `claude-lifeline-aarch64-unknown-linux-musl`
>    - Windows x86_64: `claude-lifeline-x86_64-pc-windows-msvc.exe`
> 2. Save the binary to `~/.claude/bin/claude-lifeline` (Windows: `%USERPROFILE%\.claude\bin\claude-lifeline.exe`), create the directory if it doesn't exist, and make it executable (`chmod +x`)
> 3. Edit `~/.claude/settings.json` — add or merge the following key into the existing JSON:
>    ```json
>    "statusLine": { "type": "command", "command": "~/.claude/bin/claude-lifeline" }
>    ```
>    Do NOT overwrite the entire file — preserve all existing settings.
> 4. Tell the user to restart Claude Code to activate the new status line.

### 手动安装

**macOS / Linux：**

```bash
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash
```

**Windows (PowerShell)：**

```powershell
irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex
```

安装后重启 Claude Code 即可生效。

### 从源码构建

```bash
git clone https://github.com/xjoker/claude-lifeline.git
cd claude-lifeline
cargo build --release
mkdir -p ~/.claude/bin
cp target/release/claude-lifeline ~/.claude/bin/
```

然后在 `~/.claude/settings.json` 中添加：

```json
{
  "statusLine": {
    "type": "command",
    "command": "~/.claude/bin/claude-lifeline"
  }
}
```

### 升级

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s upgrade
```

Windows：重新运行安装命令即可 — 已是最新版本时会自动跳过。

### 卸载

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s uninstall
```

```powershell
# Windows (PowerShell)
& { $env:ACTION='uninstall'; irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex }
```

## 功能详解

### 第一行 — 会话信息

```
[Opus 4.6]  my-project  git:(main* ↑2)  1h 23m
 ^^^^^^^^^   ^^^^^^^^^^      ^^^^^^^^^   ^^^^^^
 模型         项目名称        Git 状态    会话时长
```

- **模型** — Claude Code 提供的模型显示名称（如 `Sonnet 4.6`、`Opus 4.6`、`Haiku 4.5`）
- **项目名称** — 当前工作目录名
- **Git 状态** — 分支名、脏标记（`*`）、领先（`↑N` 绿色）/ 落后（`↓N` 红色）上游
- **会话时长** — 会话启动以来的经过时间，暗色显示

### 第二行 — 资源使用

```
ctx █████░░░░░ 53%  │  5h ████░░|░░░░ 34%(1h 23m)  │  7d ██|█░░░░░░░ 22%!/p14.05%(...)
^^^                    ^^                               ^^
上下文窗口              5 小时配额                       7 天配额
```

### 上下文窗口 (`ctx`)

10 格进度条显示上下文窗口使用率。

| 颜色 | 阈值 | 含义 |
|------|------|------|
| 绿色 | `< 70%` | 余量充足 |
| 黄色 | `70–85%` | 接近上限 |
| 红色 | `≥ 85%` | 即将用尽 |

当上下文使用率 **≥ 85%** 时，显示 token 用量明细：

```
ctx █████████░ 92% (in:120k c:65k)
                    ^^^^^^  ^^^^^
                    输入 token  缓存 token（创建 + 读取）
```

### 速率限制配额 (`5h` / `7d`)

每个配额段包含进度条、百分比和后缀信息：

#### 进度条

```
██|█░░░░░░░
^^|^
填充块（实际使用量）
  |
  配速标记（基于已用时间的预期位置）
```

- **`█`** — 填充块，使用配额对应颜色，数量反映实际使用百分比
- **`|`** — 配速标记（粗体白色），插入在时间窗口已过比例对应的位置。**不会替换**填充块
- **`░`** — 空白块（暗色）

#### 百分比与警告

```
22%!/p14.05%
^^^  ^^^^^^^
使用率  配速位置（仅超速时显示）
   ^
   ! = 超速警告
```

- **使用率 `%`** — 当前配额消耗百分比
- **`!`** — 当使用率超过配速 5% 以上时追加（超速状态）
- **`/p14.05%`** — 配速位置，即时间窗口已过比例。仅在超速时显示

#### 后缀：重置时间、预计耗尽、恢复时长

```
(6d 0h ETA 4/19 08:18 wait 13h)
 ^^^^^  ^^^^^^^^^^^^^^  ^^^^^^^^
 重置倒计时  预计耗尽时间   恢复等待时长
```

- **重置倒计时** — 窗口重置剩余时间：`59m`、`3h 55m`、`6d 0h`
- **`ETA`** — 按当前消耗速率，**预测**配额将于何时耗尽（本地时间）。**这不是实际重置/到期时间。** 仅在超速且预计耗尽时间早于窗口重置时显示
  - 当天：`ETA 16:30`
  - 跨天：`ETA 4/19 01:22`
- **`wait`** — 需要暂停多久，让配速追上当前使用率。仅在超速时显示
  - 示例：`wait 59m` 表示"停工约 59 分钟，消耗就会回到正常节奏"

#### 颜色阈值

| 条件 | 颜色 |
|------|------|
| 使用率 `< 75%`，配速正常 | 蓝色 |
| 使用率 `75–90%` 或超速（`!`） | 黄色 |
| 使用率 `≥ 90%` | 红色 |

### 完整示例

**正常状态 — 配速内**

```
5h ██░░░░|░░░░ 18%(3h 55m)
   ^^^^^^       ^^^ ^^^^^^^
   │             │   └─ 窗口将在 3h 55m 后重置（届时获得全新配额）
   │             └─ 5 小时配额已消耗 18%
   └─ 2 个填充块 = 18% 已用，配速标记 | 在位置 6 = 窗口已过约 60%
      使用速度低于预期 — 无警告
```

**超速状态 — 消耗过快**

```
5h █████░|░░░░ 52%!/p32.15%(2h 10m ETA 16:30 wait 45m)
   ^^^^^^       ^^^  ^^^^^^^ ^^^^^  ^^^^^^^^  ^^^^^^^^
   │             │    │       │      │         └─ 暂停约 45 分钟可恢复正常配速
   │             │    │       │      └─ 按当前消耗速率，配额将在今天 16:30 耗尽
   │             │    │       └─ 窗口将在 2h 10m 后重置
   │             │    └─ 5h 窗口仅过了 32.15%（配速位置）
   │             └─ 52% 已用 + ! = 超速警告（52% 使用 vs 32% 配速，差距 > 5%）
   └─ 5 个填充块 = 52% 已用，配速标记 | 在位置 3 = 约 32% 时间已过
      使用量超过了配速标记 — 消耗速度快于窗口允许的速率
```

**危险状态 — 接近上限**

```
5h █████████|░ 93%!/p85.00%(25m ETA 15:05 wait 12m)
   ^^^^^^^^^^      ^^^^^^^^  ^^^  ^^^^^^^  ^^^^^^^^
   │                │        │    │        └─ 暂停约 12 分钟可对齐配速
   │                │        │    └─ 按当前速率，配额将在 15:05 耗尽
   │                │        └─ 25 分钟后重置
   │                └─ 窗口已过 85%
   └─ 9 个填充块 = 93%，配速标记接近末端 — 时间和配额都快用完了
```

**7 天窗口 — 跨天 ETA**

```
7d ██|█░░░░░░░ 22%!/p14.05%(6d 0h ETA 4/19 08:18 wait 13h)
   ^^^          ^^^  ^^^^^^^ ^^^^  ^^^^^^^^^^^^^^  ^^^^^^^^
   │             │    │       │     │               └─ 停工约 13 小时可恢复正常
   │             │    │       │     └─ 预计配额耗尽：4 月 19 日 08:18
   │             │    │       └─ 窗口将在 6 天 0 小时后重置
   │             │    └─ 7 天窗口仅过了 14.05%
   │             └─ 22% 已用 + !（22% vs 14%，差距 > 5%）
   └─ 配速标记 | 在位置 1（约 14%），填充块延伸到位置 2（约 22%）
```

> **核心概念**：配速标记 `|` 代表"基于已过时间，你*应该*在的位置"。如果填充块 `█` 超过了 `|`，说明你超前于配速（过度消耗）。两者距离越远，配额消耗越激进。

## 配置

可选配置文件位于 `~/.claude/claude-lifeline/config.toml`。所有选项默认为 `true`。

```toml
[display]
context = true     # 上下文窗口段
five_hour = true   # 5 小时配额段
seven_day = true   # 7 天配额段
```

参见 [config.example.toml](../config.example.toml) 获取参考。

## 数据来源

速率限制数据按优先级解析：

| 优先级 | 来源 | 说明 |
|--------|------|------|
| 1 | `stdin.rate_limits` | Claude Code ≥ 2.1.80，无需认证 |
| 2 | 本地缓存 | `~/.claude/claude-lifeline/usage-cache.json`，5 分钟 TTL |
| 3 | API 回退 | `api.anthropic.com/api/oauth/usage`，2 秒超时 |
| 4 | 空 | 不显示配额段 |

## 性能

- **~30ms** 响应时间（远低于 Claude Code 的 500ms 限制）
- **~3MB** 发布二进制（LTO + strip）
- Git 命令、用量数据获取通过 `tokio::join!` 并发执行
- 所有二进制均为完全静态链接（Linux 使用 musl，Windows 使用静态 CRT）

## 支持平台

| 平台 | 架构 | 二进制文件 |
|------|------|-----------|
| macOS | Apple Silicon (arm64) | `claude-lifeline-aarch64-apple-darwin` |
| macOS | Intel (x86_64) | `claude-lifeline-x86_64-apple-darwin` |
| Linux | x86_64 | `claude-lifeline-x86_64-unknown-linux-musl`（静态链接） |
| Linux | ARM64 | `claude-lifeline-aarch64-unknown-linux-musl`（静态链接） |
| Windows | x86_64 | `claude-lifeline-x86_64-pc-windows-msvc.exe`（静态 CRT） |

## 更新日志

详见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

MIT — 详见 [LICENSE](../LICENSE)。
