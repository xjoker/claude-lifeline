# claude-lifeline

为 [Claude Code](https://docs.anthropic.com/en/docs/claude-code) 打造的快速 Rust 状态栏，替代默认状态条，亚 50ms 原生二进制。支持 **macOS**、**Linux** 和 **Windows**。

**[English](../README.md)**

## 预览

单行色块紧凑模式，全部信息内联。

![claude-lifeline mini 布局](img-mini.png)

```
 Opus 4.7 1M  claude-lifeline  master  +95 -442  ctx 21%  100% 3m19s  3/28%  63/68%  S:87/68%! →5/13 16:08 ↓1d8h
```

## 为什么用 claude-lifeline？

Claude Code 默认状态栏只显示基础用量百分比——它告诉不了你是否**按节奏**用得完整个窗口。

claude-lifeline 加入**配速智能**：把实际消耗速率和每个配额窗口的理想配速做对比，在你烧光之前提醒你。

### 一眼看到的信息

- **超速告警 `!`**——当实际用量超过时间已过比例的配速时，加在百分比末尾
- **耗尽时间 ETA `→HH:MM`**——按当前烧速预测多久打到 100%（仅超速时显示）
- **恢复等待 `↓Xh`**——告诉你停多久配速能追上当前用量
- **Sonnet 子配额告警**——当 Sonnet 专属用量超速时，在 7d 块旁追加一个 `S:U/P%!` 块（否则隐藏）
- **缓存 TTL 倒计时**——独立 cache 段显示命中率和预测的 prompt 缓存剩余时间，缓存死掉变红

### 其他功能

- Git 分支、dirty 状态、ahead/behind upstream
- 会话编辑量（`+lines_added` / `-lines_removed`）
- 段可配置——TOML 里开关 context / 5h / 7d / cache / edit stats
- **~30ms** 响应、**~3MB** 二进制、零运行时依赖（Linux/Windows 静态，macOS 签名）

## 安装

需要 **Claude Code ≥ 2.1.80**（用 `stdin.rate_limits`）。

### 让 AI 帮你安装（推荐）

把这个仓库 URL 给 Claude Code，它会读取下面的说明完成安装。

> **AI agents**：要安装 claude-lifeline，按以下步骤执行：
> 1. 探测用户平台，从 [latest release](https://github.com/xjoker/claude-lifeline/releases/latest) 下载对应二进制：
>    - macOS Apple Silicon: `claude-lifeline-aarch64-apple-darwin`
>    - macOS Intel: `claude-lifeline-x86_64-apple-darwin`
>    - Linux x86_64: `claude-lifeline-x86_64-unknown-linux-musl`
>    - Linux ARM64: `claude-lifeline-aarch64-unknown-linux-musl`
>    - Windows x86_64: `claude-lifeline-x86_64-pc-windows-msvc.exe`
> 2. 保存到 `~/.claude/bin/claude-lifeline`（Windows: `%USERPROFILE%\.claude\bin\claude-lifeline.exe`），目录不存在则创建，加可执行权限（`chmod +x`），macOS 上执行 `xattr -d com.apple.quarantine ~/.claude/bin/claude-lifeline` 绕过 Gatekeeper
> 3. 编辑 `~/.claude/settings.json`，把以下 key 添加或合并到已有 JSON：
>    ```json
>    "statusLine": { "type": "command", "command": "~/.claude/bin/claude-lifeline", "refreshInterval": 15 }
>    ```
>    **不要**覆盖整个文件，保留其他设置。`refreshInterval: 15` 让 cache TTL 倒计时和 quota ETA 接近实时刷新。如果用户已有自定义 `refreshInterval`，保持不动。
> 4. 告诉用户重启 Claude Code 激活新状态栏。

### 手动安装

**macOS / Linux：**

```bash
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash
```

**Windows (PowerShell)：**

```powershell
irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex
```

安装后重启 Claude Code。

### 从源码构建

一键开发安装——用 cargo 构建、部署到 `~/.claude/bin/`、合并 `settings.json`：

```bash
# macOS / Linux
git clone https://github.com/xjoker/claude-lifeline.git
cd claude-lifeline
bash install.sh dev
```

```powershell
# Windows (PowerShell)
git clone https://github.com/xjoker/claude-lifeline.git
cd claude-lifeline
$env:ACTION='dev'; .\install.ps1
```

或手动：

```bash
cargo build --release
mkdir -p ~/.claude/bin
cp target/release/claude-lifeline ~/.claude/bin/
```

然后在 `~/.claude/settings.json` 加入：

```json
{
  "statusLine": {
    "type": "command",
    "command": "~/.claude/bin/claude-lifeline",
    "refreshInterval": 15
  }
}
```

### 升级

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s upgrade
```

Windows：重跑安装命令——会自动检测，已是最新则跳过。

### 卸载

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s uninstall
```

```powershell
# Windows (PowerShell)
& { $env:ACTION='uninstall'; irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex }
```

## 布局

紧凑单行色块，全部信息内联。

```
 Opus 4.7 1M  claude-lifeline  master  +95 -442  ctx 21%  100% 3m19s  3/28%  63/68%  S:87/68%! →5/13 16:08 ↓1d8h
 ^^^^^^^^^^^  ^^^^^^^^^^^^^^^  ^^^^^^  ^^^^^^^^  ^^^^^^^  ^^^^^^^^^^  ^^^^^  ^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^
 模型         项目             Git     编辑量    Context  Cache       5h     7d      Sonnet 7d（仅超速时出现）
```

### 块说明

| 块 | 内容 | 底色 |
|-----|------|------|
| 模型 | `Opus 4.7` / `Sonnet 4.6` / `Haiku 4.5` / `Opus 4.7 1M` 等 | **按强度配色**——见下表 |
| 项目 | `cwd` basename，截断到 16 列，超出用 `..` | 灰青 |
| Git | `分支[*][ ↑N][ ↓N]`——分支名截断到 16 列 | 暖橙 |
| 编辑量 | `+lines_added -lines_removed`，来自 Claude Code 会话计数器 | 中性灰 |
| Context | `ctx N%` | **绿 / 黄 / 红** 阈值切换 |
| Cache | `hit% 剩余时间`（如 `100% 3m19s`）；缓存死掉显示 `expired` | **蓝 / 黄 / 红** |
| 5h / 7d quota | `U/P%`（如 `3/28%`）；超速追加 `!`、ETA、恢复等待 | **蓝 / 黄 / 红** 阈值 + 超速切换 |
| Sonnet 7d | `S:U/P%!`——只在 Sonnet 用量超速时出现 | 黄 / 红 |

### 模型强度配色

模型块底色反映等级：

| 模型 | 底色 | 意义 |
|------|------|------|
| `Opus` | 紫红（256 #134） | 旗舰 |
| `Sonnet` | 紫蓝（256 #99） | 平衡 |
| `Haiku` | 青蓝（256 #38） | 轻快 |
| 其他 / 未知 | 灰（256 #102） | 兜底 |

### Context 颜色阈值

| 颜色 | 阈值 |
|------|------|
| 绿 | `< 60%` |
| 黄 | `60–70%` |
| 红 | `≥ 70%` |

### Quota 颜色阈值

| 颜色 | 条件 |
|------|------|
| 蓝 | 用量 `< yellow_at` 且未超速 |
| 黄 | `yellow_at ≤ 用量 < red_at` 或超速 |
| 红 | 用量 `≥ red_at` |

默认：5h `yellow_at = 75 / red_at = 90`，7d `yellow_at = 80 / red_at = 90`。

### 超速指示

当 quota 实际用量超过时间已过的配速时：

1. 切换到黄底（≥ `red_at` 则保持红）
2. 百分比对后追加 `!`
3. 追加 ` →HH:MM`——预测耗尽时间（跨天用 `M/D HH:MM`）
4. 追加 ` ↓Xh`——停工多久能让配速追平用量

```
85/23%! →9:35 ↓2h
^^ ^^   ^^^^^ ^^^^
│  │    │     └─ 停 2h 配速能追上
│  │    └─ 预测耗尽时间 09:35
│  └─ 配速位置：5h 窗口已过 23%
└─ 5h 配额已用 85%（远超配速）
```

### Sonnet 子配额

某些 Max 套餐里 Sonnet 有独立的配额上限，`/api/oauth/usage` 的 `seven_day_sonnet` 字段单独跟踪。Sonnet 块**只在 Sonnet 用量超速时**渲染——设计上保持安静，只在你 Sonnet 烧得比 7 天窗口允许的更快时告警。

```
63/68%  S:87/68%! →5/13 16:08 ↓1d8h
^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^
│       └─ Sonnet 87%，7d 窗口才过 68%——超速
└─ 7d 总量 63%（未超速，正常蓝）
```

### Cache 段

```
100% 3m19s     # 活，命中率正常——蓝底，命中率 + 预估剩余 TTL
30%  4m12s     # 活但命中率低——黄底（缓存重建中）
expired        # 缓存刚死（检测到真过期后的 60s 窗口）——红底
```

命中率 = `cache_read / (input + cache_read + cache_creation)`，取自最近一次 API turn。剩余时间预测为 `last_active + 5min`（Anthropic 没暴露 server 端缓存状态 API）。没有可用信号时隐藏。

### 自适应宽度换行

根据终端宽度自动调整：

- **足够宽** → 全部块一行
- **窄** → 拆两行：`model + project + git + edits` 一行，`ctx + cache + 5h + 7d + sonnet` 一行
- **极窄** → 每块一行

块间 1 列空格保证相邻同色块仍可区分，无需分隔字符。

> 关于编辑量：`+X -Y` 来自 Claude Code 的 `cost.total_lines_added` / `total_lines_removed`，是会话级计数器，统计 Edit / Write 工具碰过的所有行。**不**按 `.gitignore` 过滤，**不**对应任何 `git diff`。新会话重置。

## 配置

可选配置文件 `~/.claude/claude-lifeline/config.toml`。

```toml
[display]
context    = true   # Context window 块
cache_hit  = true   # 缓存命中率 + TTL 块
five_hour  = true   # 5 小时 quota 块
seven_day  = true   # 7 天 quota 块（同时控制 Sonnet 子块）
edit_stats = true   # 来自 Claude Code 会话计数器的 +lines_added / -lines_removed

# 颜色阈值（可选——下方为默认值）
# 校验：每个 yellow_at 必须 < red_at 且在 [0, 100]；
#       非法对单独回退到默认，其他字段保持。
[thresholds]
ctx_yellow_at       = 60.0   # ctx >= 此值 → 黄
ctx_red_at          = 70.0   # ctx >= 此值 → 红

# 5h / 7d quota 独立调。7d 默认比 5h 宽松，因为更长的重置
# 窗口让中段用量不那么紧急。
five_hour_yellow_at = 75.0
five_hour_red_at    = 90.0
seven_day_yellow_at = 80.0
seven_day_red_at    = 90.0

# 超速容差（%）。pace_tolerance = 0（严格）下，用量一旦
# 超过配速线就触发 `!`。调高可吸收短期冲量，比如
# pace_tolerance = 5.0 表示"只在领先配速 >5% 时告警"。
pace_tolerance      = 0.0
```

参考 [config.example.toml](../config.example.toml)。

## 数据源

Rate limit 数据按优先级解析：

| 优先级 | 来源 | 备注 |
|--------|------|------|
| 1 | `stdin.rate_limits` | Claude Code ≥ 2.1.80，无需 auth；只提供 `five_hour` + `seven_day` |
| 2 | 本地缓存 | `~/.claude/claude-lifeline/usage-cache.json`，5min TTL；rate_limits 写入时保留 Sonnet 数据 |
| 3 | API fallback | `api.anthropic.com/api/oauth/usage`，2s 超时；提供 Sonnet 子配额 |
| 4 | 空 | quota 段不显示 |

### 凭证

API fallback 用的 OAuth token 读取来源：

1. `~/.claude/.credentials.json`（Linux / Windows / 老版 macOS）
2. **macOS Keychain** —— `security find-generic-password -s "Claude Code-credentials"` 兜底，覆盖 Claude.app 不写凭证文件的情况

## 性能

- **~30ms** 响应（远低于 Claude Code 500ms 预算）
- **~3MB** release 二进制（LTO + strip）
- Git 命令、usage 数据通过 `tokio::join!` 并发获取
- 全平台静态二进制（Linux musl，Windows static CRT）

## 支持平台

| 平台 | 架构 | 二进制 |
|------|------|--------|
| macOS | Apple Silicon (arm64) | `claude-lifeline-aarch64-apple-darwin` |
| macOS | Intel (x86_64) | `claude-lifeline-x86_64-apple-darwin` |
| Linux | x86_64 | `claude-lifeline-x86_64-unknown-linux-musl`（静态） |
| Linux | ARM64 | `claude-lifeline-aarch64-unknown-linux-musl`（静态） |
| Windows | x86_64 | `claude-lifeline-x86_64-pc-windows-msvc.exe`（静态 CRT） |

## Changelog

见 [CHANGELOG.md](CHANGELOG.md)。

## License

MIT —— 见 [LICENSE](../LICENSE)。
