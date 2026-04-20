# Changelog

All notable changes to claude-lifeline will be documented in this file.

## [Unreleased]

### Added
- **Mini layout** (`layout = "mini"`) — single-line colored-block bar with everything
  inline: `model · project · git · ctx N% · U/P% 5h · U/P% 7d`. Each segment is a
  16-color ANSI block (model=magenta, project=cyan, git=yellow, ctx green/yellow/red,
  quota blue/yellow/red), separated by a 1-column gap so adjacent same-color blocks
  stay distinguishable. ETA appears only on over-pace segments; reset/wait/token-detail
  dropped. Width-aware: single-line → identity+metrics 2-line → 1-block-per-line.
  Long project/branch names truncated to 16 columns with ASCII `..` ellipsis.

### Changed
- Context color thresholds unified to `<60 green / <70 yellow / >=70 red` (was
  `<70 / <85 / >=85`). Applies to both standard and mini layouts.
- Mini layout uses pinned 256-color RGB values for all blocks (model=99 violet,
  project=73 cadet, git=209 orange, ctx green/gold/red, quota sky-blue/gold/red),
  with `fg=232` near-black text. Bypasses terminal theme palette mapping so blocks
  render the same RGB on Windows Terminal, iTerm2, Alacritty, Kitty, gnome-terminal,
  etc. Only Win10 legacy ConHost (cmd.exe) lacks 256-color support.
  Characters limited to ASCII + Box-Drawing/Block-Elements/Arrows that ship with
  default monospace fonts on all three platforms.

## [0.0.3] - 2026-04-16

### Added
- **Auto-update detection** — checks GitHub releases once per 24h via background subprocess, shows `↑0.0.3` in status bar when a new version is available. Zero latency impact (file read only on hot path)
- **macOS ad-hoc codesign** in CI — reduces Gatekeeper warnings

### Changed
- Over-pace alert triggers immediately when usage exceeds pace (no threshold)
- Removed separator line (redundant with Claude Code's own divider)
- Install scripts: use `jq` for JSON editing when available, fix trailing comma bugs
- Install scripts: proper version comparison (`v` prefix stripped)
- Cache invalidation: resets_at past expiry now triggers re-fetch

### Fixed
- install.ps1: create `settings.json` when file doesn't exist
- Dead code warnings eliminated (warning-clean build)

## [0.0.1] - 2026-04-15

### Added
- Two-line ANSI status bar for Claude Code
- Context window progress bar with green/yellow/red thresholds (token breakdown at ≥ 85%)
- 5-hour and 7-day rate limit quota bars with pace markers
- Over-pace alerts (yellow bar + `!`, threshold ±5%)
- Depletion ETA — predicts when quota will hit 100% at current burn rate
- Recovery time — `wait Xm` showing how long to pause when over-pace
- Pace percentage — `/pXX.XX%` showing exact pace position, only when over-pace
- Session duration from transcript file creation time
- Git branch, dirty status, ahead/behind upstream
- Configuration file (`~/.claude/claude-lifeline/config.toml`) to toggle segments
- Usage data: stdin rate_limits → cache → API fallback
- Install scripts with upgrade/uninstall support (macOS, Linux, Windows)
- Static binaries for all platforms (musl on Linux, static CRT on Windows)
- `--version` flag for version detection
