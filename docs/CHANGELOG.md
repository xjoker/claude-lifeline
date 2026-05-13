# Changelog

All notable changes to claude-lifeline will be documented in this file.

## [Unreleased]

## [0.1.0] - 2026-05-13

### Added
- **Sonnet 7d quota indicator** — extra mini block appears next to the
  regular 7d block when Sonnet-specific usage exceeds pace. Format
  `S:U/P%! →HH:MM ↓Xh` (e.g., `S:87/68%! →5/13 16:08 ↓1d8h`). Source:
  `seven_day_sonnet` field from `/api/oauth/usage` (was previously
  ignored). Designed as a quiet signal — block is hidden entirely
  when Sonnet usage is within pace. Useful for plans where Sonnet has
  a separate quota cap that can be burned faster than the overall 7d
  total.
- **macOS Keychain credential fallback** — when
  `~/.claude/.credentials.json` is absent (the common case on macOS
  where Claude.app stores credentials in Keychain), `auth.rs` now
  falls back to `security find-generic-password -s "Claude Code-credentials"`.
  Failures from the keychain subprocess are reported to stderr instead
  of silently disabling the API fallback path.

### Changed
- **Mini layout is now the only layout** — removed the dual-line
  "standard" layout with progress bars, pace markers, and verbose
  suffixes. The `[display].layout` config field is gone (the `Layout`
  enum and its `auto / single / multi / mini` variants no longer
  exist). Existing `layout = "..."` lines in `config.toml` are
  ignored by TOML extra-key tolerance — no migration required.
- **Terminal width default raised from 80 → 200** — Claude.app GUI
  subprocesses have no controlling terminal, so all three width
  probes (`COLUMNS` env / `terminal_size()` on std fds / `/dev/tty`
  ioctl) return None, and the default kicks in. The previous 80
  caused ~120-column mini single-lines to wrap into two physical
  lines on wide CC displays. New default trades that for occasional
  mid-block wraps on genuinely narrow terminals.
- **Cache fields preserve Sonnet across rate_limits writes** — every
  Claude Code hook invocation writes the cache from `stdin.rate_limits`,
  which has no per-model fields. To stop the Sonnet pct/resets_at from
  being repeatedly wiped, the rate_limits write path now reads the
  existing cache first and carries over a still-fresh `seven_day_sonnet`
  entry. `is_cache_fresh()` also includes `seven_day_sonnet_resets_at`
  in its expiry check.
- **`read_cache` switched to `tokio::fs`** — was synchronous I/O in an
  async function; minor but eliminates tokio-reactor blocking and adds
  the `fs` tokio feature.

### Fixed
- **`cache_ttl::check_and_update` same-call branch never wrote state** —
  on a repeat observation of the same `cache_read` value, the function
  returned `prev.last_active_at` but skipped `write_state`, so a stale
  `last_expired_at` from a previous run would persist in the per-session
  state file. The next time `cache_read` dropped to 0 (e.g., new session
  on the same session_id), `within_expired_window` would falsely trigger
  for up to 60s. Same-call branch now writes state explicitly with
  `last_expired_at: None`.

### Added (previous)
- **Independent cache segment** — cache info (hit rate + TTL countdown)
  now renders as its own segment between `ctx` and `5h`, no longer
  embedded in the `ctx` block. Standard mode: `cache 96% 4m12s`; mini:
  a separate label-less block sized like `5h` / `7d`. Hit rate is
  `cache_read / (input + cache_read + cache_creation)` from the most
  recent API call. Color-coded for at-a-glance anomaly detection:
  - real TTL expiry (60s window) → red `expired`
  - hit rate < 30% → yellow (cache rebuild in progress)
  - normal → cyan (standard) / blue background (mini)
  Hidden when `current_usage` is null and there's no expiry to report.
  Toggle via `display.cache_hit` (default `true`). Anthropic exposes no
  server-side cache state API, so this reflects the most recent turn's
  cache behavior — not session totals.
- **Cache TTL countdown** — when cache is alive, an estimated
  remaining-life timer follows the hit rate inside the cache segment:
  `cache 96% 4m12s`. Predicts expiry as `last_observed_hit_time + 5min`,
  refreshed on every new API call. Disappears when cache_read drops to
  0 (cache died) or the predicted TTL has elapsed. State persisted at
  `~/.claude/claude-lifeline/cache-ttl-<session_id>.json`. Useful when
  returning from a long pause: visible timer means "context still
  cached, send freely"; absent timer means "next message pays
  cache_creation cost."
- **Cache decision diagnostic log** — every cache_read transition is
  classified and appended to
  `~/.claude/claude-lifeline/cache-decisions.jsonl`:
  ```json
  {"ts":...,"session":"...","category":"real_expiry|compact_or_first|new_call",
   "prev_cache_read":N,"cache_read":M,"cache_creation":X,"input":Y,
   "ratio":..., "elapsed_since_prev_secs":...}
  ```
  Lets you audit "why didn't we record a TTL sample?" — the
  `compact_or_first` rows show classification details, so the
  `cache_creation >= input * 2` heuristic can be tuned against real
  data. Same-call repeated observations are not logged (noise). Auto-
  compacts at 200KB / 1000 rows / 90 days, atomic-rename rotation.

### Added
- **TTL sample collection (Phase 1, opt-out: just delete the file)** —
  every confirmed real-expiry event records one sample to
  `~/.claude/claude-lifeline/ttl-samples.jsonl`:
  ```json
  {"ts":1778048545,"observed_ttl_secs":287,"cache_creation":150000,"input":50,"prev_cache_read":150000}
  ```
  Phase 2 (future) will use these to calibrate the TTL prediction
  against Anthropic's real behavior. Phase 1 only collects.
  Auto-compacts when file exceeds 50KB: drops samples older than 90 days,
  caps to last 200 samples. Concurrent multi-session writes are atomic
  (POSIX `O_APPEND` for lines < 4KB; compact uses tempfile + rename).
- **Real cache expiry detection (post-hoc)** — when the next API call
  after an idle period returns `cache_read = 0`, the renderer
  distinguishes two causes via the `cache_creation : input_tokens`
  ratio:
  - `cache_creation >= input_tokens * 2` → real TTL expiry, prefix
    rebuilt server-side. Cache segment shows red `expired` (standard)
    / red-background `expired` block (mini) for the next 60 seconds.
  - `cache_creation < input_tokens * 2` → likely `/compact` or new
    session, no `expired` hint shown.

  This is the only ground-truth signal Anthropic exposes — there is no
  cache-state query API. The hint tells you "the message you just sent
  paid the cache_creation premium because the cache had died."

### Fixed
- **Cache TTL countdown stuck at 5m with multiple CC terminals open** —
  state was persisted to a single `cache-ttl.json`, so each CC session's
  refresh would overwrite it with its own session_id. Subsequent reads
  treated the differing session_id as "new API call" and reset
  `last_active_at = now`, so the countdown never decreased below ~15s.
  State now lives in `cache-ttl-<session_id>.json` per session, isolated
  from concurrent terminals.

### Changed
- **Install scripts now set `refreshInterval: 15`** in `settings.json`'s
  `statusLine` block. Without this, Claude Code only re-runs the
  statusline on assistant-message / `/compact` / permission events —
  cache TTL countdowns and quota ETAs would freeze during idle. 15s is
  a balance between visual smoothness and CPU cost (~30ms per run).
  Existing user customization (e.g., `refreshInterval: 30`) is preserved
  on upgrade; the default is only applied when the field is missing.

### Removed
- **`(in:Xk c:Yk)` token detail at ctx >= 85%** — superseded by the
  always-on `cXX%` cache hit rate, which conveys the same cache-vs-fresh
  signal in 5 columns instead of 18. The `ctx_token_detail_at` threshold
  config field has been removed; existing config files with this key
  will continue to load (TOML extra-key tolerance) but the value is
  ignored.

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
