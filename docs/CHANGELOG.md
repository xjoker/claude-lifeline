# Changelog

All notable changes to claude-lifeline will be documented in this file.

## [Unreleased]

### Added
- `CLAUDE_CONFIG_DIR` environment variable is now honored — the same env var
  Claude Code itself reads. When set to a non-empty value, every derived path
  (`projects/`, `claude-lifeline/`, `.credentials.json`) re-roots there.
  Whitespace-only values fall back to the `~/.claude` default. `doctor` reports
  the resolved root and flags when the override is in effect.

## [0.4.0] - 2026-05-16

This release re-shapes `claude-lifeline` from a single-purpose statusline
binary into a multi-mode companion CLI. The default behaviour is unchanged —
running `claude-lifeline` with stdin JSON still renders the same status line
Claude Code expects, so existing `settings.json` integrations keep working
without modification.

### Added
- **Subcommand dispatch** (`clap` based). New commands:
  - `claude-lifeline statusline` — explicit form of the existing default.
  - `claude-lifeline tui` — interactive ratatui dashboard with four tabs:
    Sessions, Usage, Config, Logs. Keys: `tab` / `1`-`4` switch tabs,
    `j`/`k` or arrows navigate, `space` / `enter` toggles config flags,
    `r` refreshes, `q` / `esc` quits.
  - `claude-lifeline config {show|path|edit|init}` — inspect and edit
    `~/.claude/claude-lifeline/config.toml`.
  - `claude-lifeline update {check|run [--force]}` — query GitHub
    releases and self-replace the running binary (atomic rename on
    Unix, rename-aside on Windows). Asset picker matches
    `darwin-arm64` / `linux-x86_64` / etc.
  - `claude-lifeline doctor` — diagnostic report covering PATH, data
    dir, config presence, credentials, transcript count, and the
    statusLine entry in `~/.claude/settings.json`.
- **Data layer** (`src/data/`): scans `~/.claude/projects/**/*.jsonl`
  to summarise sessions (model, branch, token usage, last activity) and
  rolls up totals per model and per project for today / 7d / all-time.
  Multi-session statusline calls remain stateless — no shared "current
  session" file (avoids the documented overwrite anti-pattern).
- TUI Config view writes back to `config.toml` via an atomic
  `write → rename` and the regenerated TOML preserves all thresholds.

### Changed
- Crate is now structured around a richer set of dependencies: `clap`,
  `ratatui`, `crossterm`, `walkdir`, `sha2`, `futures-util`, `tempfile`.
  Release-profile size is still dominated by `reqwest`/`rustls`; expect
  a modest binary-size increase (~1 MB depending on platform).
- `read_config` and the update cache path now go through
  `data::paths::*` helpers so every consumer (statusline, TUI, doctor)
  resolves the same `~/.claude/claude-lifeline/` root.

### Security
- **Release artifacts are now checksummed.** `release.yml` writes a
  `SHA256SUMS` file alongside the platform binaries, and
  `claude-lifeline update run` fetches it before installing — abort on
  mismatch, abort on missing entry, warn-and-continue if a release lacks
  the file entirely (kept for backwards compatibility with pre-0.4.0
  releases).

### Added (post-skeleton)
- **`claude-lifeline watch [session_id]`** — `tail -f`-style live view
  of a single transcript. Pretty-prints user / assistant / tool_use /
  tool_result entries with per-event timestamps and the running token
  counter. Omitting `session_id` follows the most recently modified
  transcript across `~/.claude/projects/`. Uses a 250 ms polling loop —
  no `notify` dependency.

### Notes
- The legacy hidden flag `--check-update` is preserved for the
  background self-spawn used by the statusline path.
- The TUI deliberately re-reads transcripts each refresh rather than
  caching a "current session" file shared across terminals — that
  pattern has been observed to silently overwrite state when multiple
  Claude Code windows are open.
- Binary size on macOS arm64 (release, LTO, strip): **3.82 MB** —
  +1.8 MB over 0.3.0. The increase is dominated by `ratatui` +
  `crossterm` + `clap`. Statusline cold-start latency unchanged.

## [0.3.0] - 2026-05-16

### Added
- **Subscription badge segment** (opt-in via `display.subscription`) — parses
  `subscriptionType` + `rateLimitTier` from the existing OAuth credentials
  (`~/.claude/.credentials.json` or macOS Keychain) and renders a compact
  badge like `MAX·20x`, `MAX·5x`, `PRO`, `FREE`. No additional API call.
- **Opus 7-day sub-quota segment** (opt-in via `display.seven_day_opus`) —
  mirrors the Sonnet sub-quota: only renders when Opus usage exceeds pace,
  prefixed `O:`.
- **Extra-usage credit-pool segment** (opt-in via `display.extra_usage`) —
  surfaces the monthly paid top-up balance returned by
  `/api/oauth/usage` (`extra_usage` node) as `$5.4K/20K 27%` for USD or
  `[XYZ] used/limit pct%` for other currencies. Auto-hidden when the
  pool is not enabled on the account (`is_enabled=false`).
- `OAuthCredential` now exposes `subscription_type` and `rate_limit_tier`
  fields. `Debug` impl prints both (non-sensitive); `access_token` stays
  redacted.
- `LIFELINE_DEBUG=1` environment flag enables one-line `eprintln!` traces
  for the stdin `rate_limits` payload and `calc_pace` output. Silent by
  default.

### Fixed
- **`calc_pace` ETA guard against negative seconds** — when a window was
  already over budget, `(100 - used) / burn_rate` could yield a negative
  `secs_to_100` that, when truncated and added to `now`, produced an ETA
  in the past. Now we early-return `None` unless `secs_to_100 > 0`.

### Security
- **`extra_usage` currency field is alphanumeric-filtered + length-capped**
  (`render::sanitize_currency`, max 6 chars). The API-supplied `currency`
  was previously passed straight into the `[XYZ]` ANSI prefix; a tampered
  upstream returning `"AB\x1b[31;1mCD"` could have injected SGR control
  bytes between the block's own opening and closing escape sequences.
- **`subscription_label` output is length-bounded to 16 chars** with `..`
  suffix on overflow, so an oversized `rateLimitTier` from a malicious or
  buggy upstream cannot blow up the subscription block geometry.
- **`format_credits` + `extra_usage_block` utilization guard NaN/Infinity**.
  Anomalous IEEE-754 values from the API previously surfaced as literal
  `"NaN"` / `"inf"` in the status bar; now non-finite credit amounts render
  as `?` and non-finite utilization is clamped (or replaced by 0%) before
  threshold lookup and `%` formatting.

### Fixed
- **`parse_rate_limit_tier` preserves all underscore-separated segments**
  for both known and unknown plans (regression of new-in-0.3.0 behaviour).
  Previously the parser dropped everything past the second segment, so a
  hypothetical `default_claude_max_enterprise_20x` rendered as `MAX·enterprise`,
  silently losing the multiplier. Now joins remaining segments with `·`,
  trimmed by the 16-char label cap.

### Compatibility
- All three new display segments default to **off**, so upgrading from
  0.2.0 leaves the existing status line untouched.
- Existing cache files (`~/.claude/claude-lifeline/usage-cache.json`)
  load cleanly into the extended `CachedUsage` shape via `#[serde(default)]`.

## [0.2.0] - 2026-05-14

### Security
- **Bump `rustls-webpki` to 0.103.13** — addresses RUSTSEC-2026-0104
  (reachable panic in certificate revocation list parsing). The previous
  0.103.12 transitively pulled in by `reqwest → rustls` could be made to
  panic on a maliciously crafted certificate chain encountered during
  TLS handshake to either the Anthropic usage API or GitHub release
  endpoint.
- **stdin read now bounded to 1 MiB** — `input::read_stdin` previously
  used `read_to_string` without a size limit, so a malformed or
  malicious CC hook payload could OOM the statusline process. Real CC
  hook payloads run <10 KiB.
- **`OAuthCredential` Debug now redacts `access_token`** — replaces
  derived `Debug` with a custom impl that prints `[REDACTED]` for the
  token field. Prevents accidental leak through `{:?}` formatting,
  panic backtraces, or `anyhow` error context.
- **Update-hint version string is sanitized + length-capped (20 chars)**
  before rendering. The previous path inserted `tag_name` straight from
  GitHub (or a locally-writable `update-cache.json`) into ANSI output,
  enabling escape-sequence injection if either source was tampered with.
- **Local file reads now bounded** — `auth.rs`/`config.rs` cap at
  64 KiB / 128 KiB respectively; `usage::read_cache` uses
  `tokio::fs::File::take(128 KiB)`. Prevents symlink-to-`/dev/zero` and
  similar resource-exhaustion attacks against credential / config /
  cache paths.

### Fixed
- **`git` commands no longer run in the spawn process's cwd when stdin
  has no cwd field** — previously `unwrap_or_default()` produced `""`,
  and `Command::current_dir("")` inherits the calling process's cwd
  (the CC hook subprocess location, not the user's project). `git`
  is now passed `Option<&str>` and short-circuits to `GitInfo::default()`
  when cwd is `None` or empty.
- **Usage cache no longer flushes all windows when any single
  `resets_at` expires** — previously `is_cache_fresh` returned `false`
  the moment any of `five_hour` / `seven_day` / `seven_day_sonnet`
  passed their reset timestamp, forcing an API round-trip even though
  the other two windows were still valid. Per-window staleness is now
  filtered inside `cached_to_usage`, and the cache-level check only
  considers the overall 5-minute TTL.
- **`usage::write_cache` and `usage::read_cache` no longer use `std::fs`
  inside async contexts** — switched to `tokio::fs` to avoid blocking
  the reactor thread under the 30 ms statusline budget.

### Removed
- **Cache hit / TTL countdown segment** — the independent cache block
  (hit% + predicted remaining lifetime, plus `expired` flash) has been
  removed along with `src/cache_ttl.rs` and `src/ttl_samples.rs`. In
  practice the signal was rarely actionable: the 5-minute TTL is an
  Anthropic convention not surfaced via API, hit-rate fluctuated mostly
  with prompt size rather than user-visible behaviour, and the
  `expired` flash carried no recoverable action. The `display.cache_hit`
  config key is gone — leftover entries in `config.toml` are silently
  ignored by TOML extra-key tolerance.

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
