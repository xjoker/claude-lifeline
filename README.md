# claude-lifeline

A fast Rust status line for [Claude Code](https://docs.anthropic.com/en/docs/claude-code), replacing the default Node.js status bar with a feature-rich, sub-50ms native binary.

## Preview

```
─────────────────────────────────────────
[Sonnet 4.6 | Max]  my-project  git:(main* ↑2)  1h 23m
ctx █████░░░░░ 53%  │  5h ██|██░░░░░░ 35%/p22.50%(3h 55m)  │  7d ██|█░░░░░░░ 21%!/p12.43%(6d 3h ↑ →4/19 01:22)
```

**Line 1** — model, plan, project, git branch, session duration

**Line 2** — context window, 5-hour quota, 7-day quota with pace markers

## Features

### Context Window

- 10-block progress bar with color thresholds:
  - **Green** `< 70%` — comfortable
  - **Yellow** `70–85%` — getting close
  - **Red** `≥ 85%` — approaching limit, shows token breakdown `(in:120k c:65k)`

### Rate Limit Quotas (5h / 7d)

- **Progress bar** — filled blocks `█` showing actual usage
- **Pace marker** `|` — bold white line inserted at the expected usage position based on elapsed time in the window, does not replace filled blocks
- **Pace percentage** `/p22.50%` — the exact pace position (2 decimal places), indicating how much time has elapsed relative to the window
- **Over-pace alert** — when usage exceeds pace by more than 5%:
  - Bar color turns **yellow**
  - Percentage suffixed with `!`
  - Red `↑` arrow — consuming faster than sustainable, quota may deplete before window resets
- **Under-pace indicator** — when usage is below pace by more than 5%:
  - Green `↓` arrow — consuming slower than average, quota headroom is healthy
- **No arrow** — usage is within ±5% of pace, on track for normal consumption
- **Reset countdown** — time until window resets: `3h 55m`, `6d 3h`
- **Depletion arrow** `→` — red, shows the estimated local time when quota will hit 100% at current burn rate

### Depletion ETA

When consuming faster than sustainable (usage > pace + 5%), estimates when quota will hit 100% at current burn rate:

- Same day: `→16:30` (local time)
- Different day: `→4/19 01:22` (local time, M/D format)
- Only shown when depletion would occur **before** the window resets

### Color Thresholds (Quotas)

| Condition | Color |
|-----------|-------|
| Usage `< 75%`, on pace | Blue |
| Usage `75–90%` or over-pace | Yellow |
| Usage `≥ 90%` | Red |

### Git Status

- **Branch name** with dirty flag `*`
- **Ahead** `↑N` (green) — commits ahead of upstream
- **Behind** `↓N` (red) — commits behind upstream
- Graceful fallback when no upstream is configured

### Session Duration

- Calculated from transcript file creation time
- Displayed as `15m`, `1h 23m` at end of line 1

### Data Sources (Priority)

| Priority | Source | Notes |
|----------|--------|-------|
| 1 | `stdin.rate_limits` | Claude Code ≥ 2.1.80 |
| 2 | Local cache | `~/.claude/claude-lifeline/usage-cache.json`, 5min TTL |
| 3 | API fallback | `api.anthropic.com/api/oauth/usage`, 2s timeout |
| 4 | Empty | No quota display |

### Performance

- **~30ms** response time (well under Claude Code's 500ms budget)
- **2.8MB** release binary (LTO + strip)
- Git commands run concurrently with quota data fetch via `tokio::join!`

## Install

### One-line install (requires pre-built release)

```bash
curl -fsSL https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.sh | bash
```

### Build from source

```bash
git clone https://github.com/xjoker/claude-lifeline.git
cd claude-lifeline
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

Restart Claude Code to activate.

## Uninstall

```bash
rm ~/.claude/bin/claude-lifeline
```

Remove the `statusLine` section from `~/.claude/settings.json`.

## Supported Platforms

| Platform | Architecture |
|----------|-------------|
| macOS | Apple Silicon (arm64) |
| macOS | Intel (x86_64) |
| Linux | x86_64 |

## License

MIT
