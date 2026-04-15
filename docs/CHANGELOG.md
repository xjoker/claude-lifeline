# Changelog

All notable changes to claude-lifeline will be documented in this file.

## [0.2.0] - 2025-04-15

### Added
- **Depletion ETA** — predicts when quota will hit 100% at current burn rate, displayed as `ETA 16:30` (local time) or `ETA 4/19 01:22` (cross-day)
- **Recovery time** — when over-pace, shows `wait 59m` indicating how long to pause for pace to catch up
- **Session duration** — elapsed time from transcript file creation, shown at end of line 1
- **Token breakdown** — at context ≥ 85%, shows `(in:120k c:65k)` with input and cache token counts
- **Git ahead/behind** — `↑N` / `↓N` after branch name showing commits ahead/behind upstream
- **Pace percentage** — `/p14.05%` showing exact pace position, only displayed when over-pace
- **Configuration file** — `~/.claude/claude-lifeline/config.toml` to toggle segments (context, five_hour, seven_day, separator)
- **Separator line** — dim `─────` divider above status bar, configurable
- **Windows support** — x86_64-pc-windows-msvc with static CRT, PowerShell install script
- **Linux ARM64 support** — aarch64-unknown-linux-musl via cross-compilation
- **Static Linux binaries** — musl targets for zero-dependency deployment

### Changed
- Over/Under pace threshold tightened from ±10% to ±5%
- Progress bar pace marker now inserts instead of replacing filled blocks
- Depletion ETA only shown when over-pace (usage > pace + 5%), avoids false alarms
- Removed ↑↓ direction arrows (pace percentage conveys the same info)

### Fixed
- `resets_at` now supports both Unix timestamps (i64) and ISO 8601 strings
- Empty suffix no longer renders `( )` when reset time is past
- 5h segment no longer disappears after window reset
- macOS CI updated from retired macos-13 to macos-14

## [0.1.0] - 2025-04-15

### Added
- Initial release
- Two-line ANSI status bar for Claude Code
- Context window progress bar with green/yellow/red thresholds
- 5-hour and 7-day rate limit quota bars with pace markers
- Over-pace alerts (yellow bar + `!`)
- Git branch and dirty status with 500ms timeout
- Plan name from credentials file (Max/Pro/Team)
- Usage data: stdin rate_limits → cache → API fallback
- Cache file with 5-minute TTL
- API fallback via `api.anthropic.com/api/oauth/usage`
- Release binary ~2.7MB, response time ~30ms
