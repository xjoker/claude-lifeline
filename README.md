# claude-lifeline

A fast Rust status line for [Claude Code](https://docs.anthropic.com/en/docs/claude-code), replacing the default status bar with a feature-rich, sub-50ms native binary. Supports **macOS**, **Linux**, and **Windows**.

**[中文文档](docs/README_CN.md)**

## Preview

**Standard layout** — full information density across two lines

![claude-lifeline standard layout](docs/img.png)

```
[Opus 4.6]  ~/Developer/Repos/my-project  git:(main* ↑2)  1h 23m
ctx █████░░░░░ 53%  │  5h ████░░|░░░░ 34%(1h 23m)  │  7d ██|█░░░░░░░ 22%!/p14.05%(6d 0h ETA 4/19 08:18 wait 13h)
```

**Mini layout** — single-line color-block bar (`layout = "mini"`)

![claude-lifeline mini layout](docs/img-mini.png)

```
 Opus  claude-lifeline  master  ctx 21%  28/51% 5h  33/44% 7d
```

## Why claude-lifeline?

Claude Code's default status bar shows basic usage percentages — but that tells you nothing about whether you're *on track* to last the full window.

claude-lifeline adds **pace intelligence**: a visual system that compares your actual consumption rate against the ideal pace for each quota window, and warns you before you burn out.

### What you get at a glance

- **Pace marker `|`** in the progress bar — shows where you *should* be based on elapsed time. When your filled blocks `█` extend past `|`, you're consuming too fast
- **Over-pace alert `!`** — triggers when usage exceeds pace by more than 5%, turns the bar yellow so you notice immediately
- **Depletion ETA** — predicts the exact local time your quota hits 100% at current burn rate (e.g., `ETA 16:30`). Only appears when you're actually at risk
- **Recovery wait time** — tells you how long to pause so your pace catches up (e.g., `wait 45m` = stop for 45 minutes and you're back on track)
- **Context token breakdown** — when context window reaches 85%+, shows input vs cache token counts so you know what's eating your context

### Also included

- Git branch, dirty status, ahead/behind upstream
- Session duration timer
- Configurable segments — toggle context, 5h quota, 7d quota on/off via TOML config
- **~30ms** response, **~3MB** binary, zero runtime dependencies (static on Linux/Windows, signed on macOS)

## Install

Requires **Claude Code ≥ 2.1.80** (for `stdin.rate_limits` data).

### Let AI install it for you (Recommended)

Share this repo URL with Claude Code — it will read the instructions below and complete the installation.

> **AI agents**: To install claude-lifeline, follow these steps:
> 1. Detect the user's platform and download the correct binary from the [latest release](https://github.com/xjoker/claude-lifeline/releases/latest):
>    - macOS Apple Silicon: `claude-lifeline-aarch64-apple-darwin`
>    - macOS Intel: `claude-lifeline-x86_64-apple-darwin`
>    - Linux x86_64: `claude-lifeline-x86_64-unknown-linux-musl`
>    - Linux ARM64: `claude-lifeline-aarch64-unknown-linux-musl`
>    - Windows x86_64: `claude-lifeline-x86_64-pc-windows-msvc.exe`
> 2. Save the binary to `~/.claude/bin/claude-lifeline` (Windows: `%USERPROFILE%\.claude\bin\claude-lifeline.exe`), create the directory if it doesn't exist, make it executable (`chmod +x`), and on macOS run `xattr -d com.apple.quarantine ~/.claude/bin/claude-lifeline` to bypass Gatekeeper
> 3. Edit `~/.claude/settings.json` — add or merge the following key into the existing JSON:
>    ```json
>    "statusLine": { "type": "command", "command": "~/.claude/bin/claude-lifeline" }
>    ```
>    Do NOT overwrite the entire file — preserve all existing settings.
> 4. Tell the user to restart Claude Code to activate the new status line.

### Manual install

**macOS / Linux:**

```bash
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex
```

Restart Claude Code after installation.

### Build from source

One-shot dev install — builds with cargo, deploys to `~/.claude/bin/`, and merges `settings.json`:

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

Or manual:

```bash
cargo build --release
mkdir -p ~/.claude/bin
cp target/release/claude-lifeline ~/.claude/bin/
```

Then add to `~/.claude/settings.json`:

```json
{
  "statusLine": {
    "type": "command",
    "command": "~/.claude/bin/claude-lifeline"
  }
}
```

### Upgrade

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s upgrade
```

Windows: re-run the install command — it auto-detects and skips if already up to date.

### Switch Layout

```bash
# macOS / Linux — switch to the compact mini layout (single-line color blocks)
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s mini
# back to the standard 2-line layout
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s standard
```

```powershell
# Windows
& { $env:ACTION='mini';     irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex }
& { $env:ACTION='standard'; irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex }
```

These run a full install (auto-upgrades the binary if a newer release is available, skips the download if already up to date), then write the layout to `~/.claude/claude-lifeline/config.toml` while preserving any other settings.

### Uninstall

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s uninstall
```

```powershell
# Windows (PowerShell)
& { $env:ACTION='uninstall'; irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex }
```

## Features

### Line 1 — Session Info

```
[Opus 4.6]  my-project  git:(main* ↑2)  1h 23m
 ^^^^^^^^^   ^^^^^^^^^^      ^^^^^^^^^   ^^^^^^
 Model       Project name   Git status   Session duration
```

- **Model** — display name from Claude Code (e.g., `Sonnet 4.6`, `Opus 4.6`, `Haiku 4.5`)
- **Project name** — current working directory name
- **Git status** — branch, dirty flag (`*`), ahead (`↑N` green) / behind (`↓N` red) upstream
- **Session duration** — elapsed time since session start, shown in dim text

### Line 2 — Resource Usage

```
ctx █████░░░░░ 53%  │  5h ████░░|░░░░ 34%(1h 23m)  │  7d ██|█░░░░░░░ 22%!/p14.05%(...)
^^^                    ^^                               ^^
Context window         5-hour quota                     7-day quota
```

### Context Window (`ctx`)

10-block progress bar showing context window usage.

| Color | Threshold | Meaning |
|-------|-----------|---------|
| Green | `< 60%` | Comfortable headroom |
| Yellow | `60–70%` | Getting close |
| Red | `≥ 70%` | Approaching limit |

When context reaches **≥ 85%**, a token breakdown appears:

```
ctx █████████░ 92% (in:120k c:65k)
                    ^^^^^^  ^^^^^
                    Input    Cache (creation + read)
```

### Rate Limit Quotas (`5h` / `7d`)

Each quota segment contains a progress bar, percentage, and suffix info:

#### Progress Bar

```
██|█░░░░░░░
^^|^
Filled blocks (actual usage)
  |
  Pace marker (expected position based on elapsed time)
```

- **`█`** — filled blocks in quota color, count reflects actual usage percentage
- **`|`** — pace marker (bold white), inserted at the position representing how much time has elapsed in the window. It does **not** replace filled blocks
- **`░`** — empty blocks (dim)

#### Percentage & Alerts

```
22%!/p14.05%
^^^  ^^^^^^^
Usage  Pace position (only shown when over-pace)
   ^
   ! = over-pace alert
```

- **Usage `%`** — current quota consumption
- **`!`** — appended when usage exceeds pace by more than 5% (over-pace)
- **`/p14.05%`** — pace position, i.e., how much time has elapsed relative to the total window. Only displayed when over-pace

#### Suffix: Reset, ETA, Recovery

```
(6d 0h ETA 4/19 08:18 wait 13h)
 ^^^^^  ^^^^^^^^^^^^^^  ^^^^^^^^
 Reset   Depletion ETA   Recovery time
```

- **Reset countdown** — time until the window resets: `59m`, `3h 55m`, `6d 0h`
- **`ETA`** — **predicted** local time when quota will hit 100% at current burn rate. **This is NOT the actual reset/expiration time.** Only shown when over-pace and depletion would occur before window reset
  - Same day: `ETA 16:30`
  - Cross-day: `ETA 4/19 01:22`
- **`wait`** — how long you need to pause for your pace to catch up to current usage level. Only shown when over-pace
  - Example: `wait 59m` means "stop for ~59 minutes and your consumption will be back on track"

#### Color Thresholds

| Condition | Color |
|-----------|-------|
| Usage `< 75%`, on pace | Blue |
| Usage `75–90%` or over-pace (`!`) | Yellow |
| Usage `≥ 90%` | Red |

### Complete Examples

**Normal — within pace**

```
5h ██░░░░|░░░░ 18%(3h 55m)
   ^^^^^^       ^^^ ^^^^^^^
   │             │   └─ Window resets in 3h 55m (you get a fresh quota then)
   │             └─ 18% of 5h quota consumed
   └─ 2 filled blocks = 18% used, pace marker | at position 6 = ~60% of window elapsed
      You're using slower than expected — no alerts
```

**Over-pace — burning too fast**

```
5h █████░|░░░░ 52%!/p32.15%(2h 10m ETA 16:30 wait 45m)
   ^^^^^^       ^^^  ^^^^^^^ ^^^^^  ^^^^^^^^  ^^^^^^^^
   │             │    │       │      │         └─ Stop for ~45min to get back on pace
   │             │    │       │      └─ At this burn rate, quota hits 100% by 16:30 today
   │             │    │       └─ Window resets in 2h 10m
   │             │    └─ Only 32.15% of the 5h window has elapsed (pace position)
   │             └─ 52% used + ! = over-pace alert (52% usage vs 32% pace, gap > 5%)
   └─ 5 filled blocks = 52% used, pace marker | at position 3 = ~32% time elapsed
      Usage is ahead of the pace marker — you're consuming faster than the window allows
```

**Critical — approaching limit**

```
5h █████████|░ 93%!/p85.00%(25m ETA 15:05 wait 12m)
   ^^^^^^^^^^      ^^^^^^^^  ^^^  ^^^^^^^  ^^^^^^^^
   │                │        │    │        └─ Pause ~12min to align with pace
   │                │        │    └─ At this rate, quota depletes by 15:05
   │                │        └─ Resets in 25 minutes
   │                └─ 85% of the window has passed
   └─ 9 filled blocks = 93%, pace marker near end — almost out of time AND quota
```

**7-day window — cross-day ETA**

```
7d ██|█░░░░░░░ 22%!/p14.05%(6d 0h ETA 4/19 08:18 wait 13h)
   ^^^          ^^^  ^^^^^^^ ^^^^  ^^^^^^^^^^^^^^  ^^^^^^^^
   │             │    │       │     │               └─ Stop for ~13h to realign
   │             │    │       │     └─ Projected depletion: April 19 at 08:18
   │             │    │       └─ Window resets in 6 days 0 hours
   │             │    └─ Only 14.05% of the 7-day window has elapsed
   │             └─ 22% used + ! (22% vs 14%, gap > 5%)
   └─ Pace marker | at position 1 (~14%), filled blocks reach position 2 (~22%)
```

> **Key concept**: The pace marker `|` represents "where you *should* be" based on elapsed time. If filled blocks `█` extend past `|`, you're ahead of pace (over-consuming). The further apart they are, the more aggressively you're burning quota.

## Mini Layout

A compact single-line variant. Everything you'd see across two lines in the standard layout is collapsed into colored blocks side-by-side. Enable it with:

```bash
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash -s mini
```

Or set `layout = "mini"` in `~/.claude/claude-lifeline/config.toml`.

![mini layout breakdown](docs/img-mini.png)

```
 Opus  claude-lifeline  master  ctx 21%  28/51% 5h  33/44% 7d
 ^^^^  ^^^^^^^^^^^^^^^  ^^^^^^  ^^^^^^^  ^^^^^^^^^  ^^^^^^^^^
 Model Project          Git     Context  5-hour     7-day
```

### Block Breakdown

| Block | Content | Background colour |
|-------|---------|-------------------|
| Model | `Opus` / `Sonnet` / `Haiku` / first word of unknown model | **Intensity-coded** — see table below |
| Project | `cwd` basename, truncated to 16 cols with `..` if longer | Cadet teal |
| Git | `branch[*][ ↑N][ ↓N]` — branch name truncated to 16 cols | Warm orange |
| Context | `ctx N%` | **Green / Yellow / Red** by threshold |
| 5h / 7d quota | `used%/pace% Lh` (e.g., `28/51% 5h`); over-pace adds `!` and ` ETA HH:MM` | **Blue / Yellow / Red** by threshold + over-pace |

### Model Intensity Colors

The model block hue reflects tier:

| Model | Background | Meaning |
|-------|-----------|---------|
| `Opus` | Violet-magenta (256 #134) | Flagship — most capable |
| `Sonnet` | Violet (256 #99) | Balanced |
| `Haiku` | Cyan (256 #38) | Light & fast |
| Other / unknown | Gray (256 #102) | Fallback |

### Context Color Thresholds (mini & standard)

| Color | Threshold |
|-------|-----------|
| Green | `< 60%` |
| Yellow | `60–70%` |
| Red | `≥ 70%` |

### Quota Color Thresholds (mini)

| Color | Condition |
|-------|-----------|
| Blue | usage `< 75%` AND on pace |
| Yellow | usage `75–90%` OR over-pace |
| Red | usage `≥ 90%` |

### Over-Pace Indicator

When a quota's actual usage exceeds the elapsed-time pace, the block:

1. Switches to yellow (or stays red if already `≥ 90%`)
2. Appends `!` after the percentage pair
3. Appends ` ETA HH:MM` showing the projected depletion time (cross-day uses `M/D HH:MM`)

```
85/23%! 5h ETA 09:35
^^ ^^   ^   ^^^^^^^^
│  │    │   └─ Predicted depletion at 09:35 local time
│  │    └─ Over-pace alert
│  └─ Pace position: 23% of the 5h window has elapsed
└─ 85% of the 5h quota consumed (way ahead of pace)
```

### Width-Aware Wrapping

Mini auto-adapts to the terminal width:

- **Wide enough** → all blocks on one line
- **Narrow** → splits into two lines: `model + project + git` on line 1, `ctx + 5h + 7d` on line 2
- **Very narrow** → one block per line

The 1-column gap between blocks ensures adjacent same-color segments stay distinguishable without separator characters.

### Differences vs Standard

Mini drops the following to keep the bar compact:

- Progress bars (`█████░░░░░`)
- Pace marker (`|`)
- Token breakdown (`(in:Xk c:Yk)`)
- Window reset countdown (`6d 0h`)
- Recovery wait time (`wait 45m`)
- Pace position (`/p32.15%`)
- Session duration

If you want any of those, use the standard layout instead.

## Configuration

Optional config file at `~/.claude/claude-lifeline/config.toml`.

```toml
[display]
context = true     # Context window segment
five_hour = true   # 5-hour quota segment
seven_day = true   # 7-day quota segment
layout = "auto"    # Layout: auto | single | multi | mini
                   #   auto   — detect terminal width; wrap per-segment if too narrow
                   #   single — always single line (may be truncated)
                   #   multi  — always one segment per line
                   #   mini   — compact colored-block single line; everything inline,
                   #            shows `model project git ctx N% U/P% 5h U/P% 7d`,
                   #            block bg switches green/yellow/red on threshold/over-pace

# Color thresholds (optional — defaults shown below)
# Validation: each yellow_at must be < red_at and within [0, 100];
#             invalid pairs fall back to the defaults below while other
#             fields stay as-is.
[thresholds]
ctx_yellow_at       = 60.0   # ctx >= this → yellow
ctx_red_at          = 70.0   # ctx >= this → red
ctx_token_detail_at = 85.0   # ctx >= this → show (in:Xk c:Yk) in standard layout

# 5h / 7d quotas are tuned independently. 7d defaults are looser than 5h
# because the longer reset window makes mid-range usage less urgent.
five_hour_yellow_at = 75.0
five_hour_red_at    = 90.0
seven_day_yellow_at = 80.0
seven_day_red_at    = 90.0

# Over-pace tolerance in percent. With pace_tolerance = 0 (strict), any
# usage above the elapsed-time pace marker triggers the `!` alert. Raise
# it to absorb short-lived bursts without flipping the whole segment
# yellow — e.g., pace_tolerance = 5.0 means "only alert when we're >5%
# ahead of pace".
pace_tolerance      = 0.0
```

Both mini and standard layouts read the same `[thresholds]` values. Mini ignores `ctx_token_detail_at` (it never renders the token breakdown).

See [config.example.toml](config.example.toml) for reference.

## Data Sources

Rate limit data is resolved in priority order:

| Priority | Source | Notes |
|----------|--------|-------|
| 1 | `stdin.rate_limits` | Claude Code ≥ 2.1.80, no auth needed |
| 2 | Local cache | `~/.claude/claude-lifeline/usage-cache.json`, 5min TTL |
| 3 | API fallback | `api.anthropic.com/api/oauth/usage`, 2s timeout |
| 4 | Empty | Quota segments not displayed |

## Performance

- **~30ms** response time (well under Claude Code's 500ms budget)
- **~3MB** release binary (LTO + strip)
- Git commands, usage data fetch run concurrently via `tokio::join!`
- All binaries are fully static (musl on Linux, static CRT on Windows)

## Supported Platforms

| Platform | Architecture | Binary |
|----------|-------------|--------|
| macOS | Apple Silicon (arm64) | `claude-lifeline-aarch64-apple-darwin` |
| macOS | Intel (x86_64) | `claude-lifeline-x86_64-apple-darwin` |
| Linux | x86_64 | `claude-lifeline-x86_64-unknown-linux-musl` (static) |
| Linux | ARM64 | `claude-lifeline-aarch64-unknown-linux-musl` (static) |
| Windows | x86_64 | `claude-lifeline-x86_64-pc-windows-msvc.exe` (static CRT) |

## Changelog

See [docs/CHANGELOG.md](docs/CHANGELOG.md).

## License

MIT — see [LICENSE](LICENSE).
