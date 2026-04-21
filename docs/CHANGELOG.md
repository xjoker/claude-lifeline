# Changelog

All notable changes to claude-lifeline will be documented in this file.

## [0.0.6] - 2026-04-21

### Changed
- **Mini quota blocks minimized** — dropped the trailing `5h` / `7d`
  label (the two blocks are always in that order, so position alone
  identifies them), replaced `ETA ` with `→` and `wait ` with `↓`, and
  stripped leading zeros from times (`9:26` not `09:26`). Over-pace
  block went from `90/80%! 5h ETA 16:56 wait 29m` (31 cols) to
  `90/80%! →16:56 ↓29m` (19 cols); a normal quota block is now `40/80%`
  (8 cols).

### Fixed
- **Terminal width detection when Claude Code pipes stdin/stdout** —
  `terminal_size()` could not locate a tty via any of the std fds in
  the statusline subprocess, so the default fell back to 120 columns
  and the mini-layout wrap logic never fired on ~80-col terminals.
  Added a `/dev/tty` fallback (Unix only) via `terminal_size_of`, and
  lowered the final hard default from 120 to 80 so width-unknown
  machines fail closed on wrapping rather than truncating output.
- **`!` and `↓wait` out of sync for sub-percent over-pace** —
  `calc_pace()` flagged any positive overrun as Over but dropped
  `recovery_secs` when fractional-second math truncated to zero,
  producing `50/50%!` with no `↓` hint on very small deltas. Over
  now emits `Some(max(secs, 1))` so the two signals always pair.

## [0.0.5] - 2026-04-20

### Added
- **Configurable color thresholds** — new `[thresholds]` section in
  `config.toml` lets you tune when ctx / quota blocks switch colour and
  how strict the over-pace alert is. 5h and 7d quotas are tuned
  independently; the 7d defaults are looser (yellow at 80% instead of
  75%) to reflect the longer reset window. Fields: `ctx_yellow_at`,
  `ctx_red_at`, `ctx_token_detail_at`, `five_hour_yellow_at`,
  `five_hour_red_at`, `seven_day_yellow_at`, `seven_day_red_at`,
  `pace_tolerance`. All fields are optional; invalid pairs
  (yellow ≥ red or out of [0, 100]) fall back per-pair to defaults.
  Mini and standard layouts share the same thresholds.
- **Session edit stats** — new segment showing `+lines_added -lines_removed`
  whenever either is non-zero. `+N` is rendered in green and `-N` in red
  so greenfield work vs refactors reads at a glance. Mini layout places
  them in a standalone neutral-gray block after git; standard layout
  appends them dim on line 1. Toggle via `display.edit_stats` (default
  true). Abbreviates to `k` at ≥1000 lines with one decimal up to 10k,
  integer k thereafter.

### Changed
- Mini layout now preserves the full `display_name` Claude Code
  provides (e.g., `Opus 4.7`, `Sonnet 4.6`, `GLM-4.5`) instead of
  collapsing it to a single keyword. The verbose `(1M context)` suffix
  is compressed to ` 1M` so the block stays compact. Tier-colour
  matcher uses `contains()` so versioned names (`Opus 4.7`) still
  colour correctly; unrecognised models fall back to gray.
- `install.sh mini` / `install.sh standard` (and their PowerShell
  equivalents) now run the full install flow first — downloading the
  latest binary when it's outdated or missing — before writing the
  layout. Previously they only edited `config.toml`, which silently
  no-op'd on machines whose binary predated the new layout value. The
  download is skipped when the binary is already current.

### Fixed
- **ANSI injection via stdin** — `display_name`, `cwd` and git branch
  names now strip all control characters (ESC, CR, LF, NUL, other
  C0/C1) before being written to stdout. Without this, a corrupted or
  hostile value could break out of its block with `\n` or inject
  arbitrary colouring with `\x1b[...]m`.
- **Update-check spawn race** — first install and every 24h cache
  expiry used to trigger a re-spawn on every ~300ms invocation while
  the background check was in flight, piling up 15+ concurrent
  subprocesses each doing a 5s network fetch. A sentinel cache is now
  written synchronously before the spawn, so subsequent invocations
  see fresh cache and skip the re-spawn; the background process still
  overwrites the sentinel with the real `latest_version` on completion.
- **install.sh no-jq fallback on empty `settings.json`** — the sed
  pattern used to produce invalid JSON (`{,"statusLine":...}`) when
  the file was `{}`. Empty-object case now writes a full fresh
  document; non-empty objects still get the comma-prefixed insertion.
- **install.sh / install.ps1 `set_layout` scope** — both scripts used
  a global pattern that would rewrite any `layout =` line anywhere in
  `config.toml`, so a future `[thresholds]` or other section with the
  same key name would have been corrupted. Replacement is now scoped
  to the `[display]` section via an awk / PowerShell state machine.
- Minor: corrected a stale comment in `usage.rs` that claimed cache
  writes were async — they are and always were synchronous.

## [0.0.4] - 2026-04-20

### Fixed
- **Update-check spawn loop** — when GitHub API was unreachable, every status-line
  invocation (~300ms) re-spawned a `--check-update` subprocess because the cache
  file was never written. `do_update_check` now writes the cache with the current
  version on failure so the 24h backoff applies even when offline. (src/update.rs)
- **Version comparison off-by-string** — auto-update prompt compared versions
  lexicographically, so `0.0.10` was treated as older than `0.0.4`. Now parses
  `X.Y.Z` into a `(u32, u32, u32)` tuple. (src/update.rs)
- **Windows credentials/cache paths** — `auth.rs` and `usage.rs` only consulted
  `$HOME`, so credential read and usage cache were broken on Windows. Both now
  fall back to `%USERPROFILE%`. (src/auth.rs, src/usage.rs)
- `PaceDirection` doc claimed a 10% tolerance the implementation never had. Doc
  updated to match strict `used > pace` semantics. (src/usage.rs)
- Removed unused `_CACHE_TTL_FAILURE` constant. (src/usage.rs)

### Added
- **Mini layout** (`layout = "mini"`) — single-line colored-block bar with everything
  inline: `model · project · git · ctx N% · U/P% 5h · U/P% 7d`. Each segment is a
  256-color block separated by a 1-column gap. Width-aware: single-line →
  identity+metrics 2-line → 1-block-per-line. Long project/branch names truncated
  to 16 columns with ASCII `..` ellipsis. Over-pace segments append `!` and
  ` ETA HH:MM`; reset / wait / token-detail dropped in mini.
- **Model intensity colors** (mini) — Opus violet-magenta (134), Sonnet violet (99),
  Haiku cyan (38), other gray (102), reflecting tier strength.
- **CWD hierarchy** in standard layout — line 1 now shows the full path with
  `$HOME` collapsed to `~` (e.g., `~/Developer/Repos/claude-lifeline`) instead of
  just the project basename.
- **Install script layout subcommands** — `install.sh mini` / `install.sh standard`
  (or `$env:ACTION='mini'` on PowerShell) edit
  `~/.claude/claude-lifeline/config.toml` to switch layout while preserving other
  settings.

### Changed
- Context color thresholds unified to `<60 green / <70 yellow / >=70 red` (was
  `<70 / <85 / >=85`). Applies to both standard and mini layouts.
- Mini layout uses pinned 256-color RGB values for all blocks with `fg=232`
  near-black text. Bypasses terminal theme palette mapping so blocks render the
  same RGB on Windows Terminal, iTerm2, Alacritty, Kitty, gnome-terminal, etc.
  Only Win10 legacy ConHost (cmd.exe) lacks 256-color support. Characters limited
  to ASCII + Box-Drawing / Block-Elements / Arrows that ship with default
  monospace fonts on all three platforms.

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
