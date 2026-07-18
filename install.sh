#!/bin/bash
set -euo pipefail

REPO="xjoker/claude-lifeline"
INSTALL_DIR="$HOME/.claude/bin"
BIN_NAME="claude-lifeline"
SETTINGS="$HOME/.claude/settings.json"
STATUS_LINE_CMD="~/.claude/bin/claude-lifeline"
# refreshInterval=15 让 statusline 在 idle 时也能及时刷新。
# 15s 在视觉流畅度和 CPU 开销之间取平衡（statusline 单跑约 30ms）
DEFAULT_REFRESH_INTERVAL=15
STATUS_LINE_JSON='{"type":"command","command":"~/.claude/bin/claude-lifeline","refreshInterval":15}'

# ── JSON helpers (jq preferred, sed fallback) ──

has_jq() { command -v jq &>/dev/null; }

settings_backup() {
  local BACKUP LOCK
  BACKUP="$SETTINGS.backup-$(date +%Y%m%d-%H%M%S)"
  LOCK="$BACKUP.lock"
  while true; do
    if mkdir "$LOCK" 2>/dev/null; then
      if [ ! -e "$BACKUP" ]; then
        break
      fi
      rmdir "$LOCK"
    fi
    sleep 1
    BACKUP="$SETTINGS.backup-$(date +%Y%m%d-%H%M%S)"
    LOCK="$BACKUP.lock"
  done
  if ! cp "$SETTINGS" "$BACKUP"; then
    rmdir "$LOCK"
    return 1
  fi
  rmdir "$LOCK"
  printf '%s\n' "$BACKUP"
}

cleanup_settings_backups() {
  local BACKUPS=("$SETTINGS".backup-*)
  [ -e "${BACKUPS[0]}" ] || return
  local REMOVE_COUNT=$((${#BACKUPS[@]} - 5))
  if [ "$REMOVE_COUNT" -gt 0 ]; then
    local INDEX
    for ((INDEX = 0; INDEX < REMOVE_COUNT; INDEX++)); do
      rm -f -- "${BACKUPS[$INDEX]}"
    done
    echo "Removed $REMOVE_COUNT old settings.json backup(s); retained 5"
  fi
}

settings_add() (
  local BACKUP TMP_SETTINGS
  BACKUP=$(settings_backup)
  TMP_SETTINGS=$(mktemp "$(dirname "$SETTINGS")/.settings.json.update.XXXXXX")
  trap 'rm -f "$TMP_SETTINGS"' EXIT
  if has_jq; then
    # 保留用户已有的 refreshInterval（如果他们手动调过），否则用默认 15s
    jq --arg cmd "$STATUS_LINE_CMD" --argjson def "$DEFAULT_REFRESH_INTERVAL" '
      .statusLine = ((.statusLine // {}) + {
        type: "command",
        command: $cmd,
        refreshInterval: (.statusLine.refreshInterval // $def)
      })
    ' "$BACKUP" > "$TMP_SETTINGS"
  else
    # sed fallback: 区分空对象 {} 与已有键的情况
    #   空对象：`{,"statusLine":...}` 会是无效 JSON，需要不带逗号的形式
    #   有键：在最后 } 前插入 `,"statusLine":...`
    cp "$BACKUP" "$TMP_SETTINGS"
    if grep -q '"' "$TMP_SETTINGS"; then
      sed -i.tmp "s|}[[:space:]]*\$|,\"statusLine\":{\"type\":\"command\",\"command\":\"$STATUS_LINE_CMD\",\"refreshInterval\":$DEFAULT_REFRESH_INTERVAL}}|" "$TMP_SETTINGS"
    else
      printf '{"statusLine":{"type":"command","command":"%s","refreshInterval":%d}}\n' "$STATUS_LINE_CMD" "$DEFAULT_REFRESH_INTERVAL" > "$TMP_SETTINGS"
    fi
    rm -f "$TMP_SETTINGS.tmp"
  fi
  mv -f "$TMP_SETTINGS" "$SETTINGS"
  cleanup_settings_backups
  echo "Updated settings.json (backup: $(basename "$BACKUP"))"
)

settings_remove() (
  if ! grep -q '"statusLine"' "$SETTINGS" 2>/dev/null; then
    echo "No statusLine config found in settings.json"
    return
  fi
  if ! has_jq; then
    echo "Error: jq is required to safely remove \"statusLine\" from $SETTINGS" >&2
    return 1
  fi
  local BACKUP TMP_SETTINGS
  BACKUP=$(settings_backup)
  TMP_SETTINGS=$(mktemp "$(dirname "$SETTINGS")/.settings.json.update.XXXXXX")
  trap 'rm -f "$TMP_SETTINGS"' EXIT
  jq 'del(.statusLine)' "$BACKUP" > "$TMP_SETTINGS"
  mv -f "$TMP_SETTINGS" "$SETTINGS"
  cleanup_settings_backups
  echo "Removed statusLine from settings.json (backup: $(basename "$BACKUP"))"
)

settings_has() {
  # 仅当 command 匹配 AND refreshInterval 已存在时才认为"已配置"。
  # 缺 refreshInterval 的旧安装会 fall through 到 settings_add，自动补上
  if has_jq; then
    jq -e --arg cmd "$STATUS_LINE_CMD" \
      '.statusLine.command == $cmd and (.statusLine.refreshInterval // 0) > 0' \
      "$SETTINGS" >/dev/null 2>&1
  else
    grep -q "\"command\".*\"$STATUS_LINE_CMD\"" "$SETTINGS" 2>/dev/null \
      && grep -q '"refreshInterval"' "$SETTINGS" 2>/dev/null
  fi
}

# ── Install 流程：下载最新二进制 + 配 settings.json（幂等，等同 upgrade） ──

do_install() {
  # 平台检测
  local OS ARCH TARGET
  OS=$(uname -s | tr '[:upper:]' '[:lower:]')
  ARCH=$(uname -m)
  case "$OS-$ARCH" in
    darwin-arm64)    TARGET="aarch64-apple-darwin" ;;
    darwin-x86_64)   TARGET="x86_64-apple-darwin" ;;
    linux-x86_64)    TARGET="x86_64-unknown-linux-musl" ;;
    linux-aarch64)   TARGET="aarch64-unknown-linux-musl" ;;
    *)
      echo "Error: unsupported platform $OS-$ARCH"
      exit 1
      ;;
  esac
  echo "Platform: $OS/$ARCH -> $TARGET"

  # 拉最新版本号
  local LATEST LATEST_VER
  LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
  if [ -z "$LATEST" ]; then
    echo "Error: failed to fetch latest release"
    exit 1
  fi
  LATEST_VER="${LATEST#v}"

  # 已是最新就跳过下载（节省带宽，仍会更新 settings.json）
  if [ -x "$INSTALL_DIR/$BIN_NAME" ]; then
    local CURRENT
    CURRENT=$("$INSTALL_DIR/$BIN_NAME" --version 2>/dev/null || echo "unknown")
    echo "Current: $CURRENT, Latest: $LATEST"
    if [ "$CURRENT" = "$BIN_NAME $LATEST_VER" ]; then
      echo "Binary already up to date."
    else
      _download_binary "$LATEST" "$TARGET" "$OS"
    fi
  else
    _download_binary "$LATEST" "$TARGET" "$OS"
  fi

  # 配 settings.json
  if [ -f "$SETTINGS" ]; then
    if settings_has; then
      echo "settings.json already configured"
    else
      settings_add
    fi
  else
    mkdir -p "$(dirname "$SETTINGS")"
    printf '{\n  "statusLine": {"type": "command", "command": "%s", "refreshInterval": %d}\n}\n' \
      "$STATUS_LINE_CMD" "$DEFAULT_REFRESH_INTERVAL" > "$SETTINGS"
    echo "Created $SETTINGS"
  fi
}

_download_binary() {
  local LATEST="$1" TARGET="$2" OS="$3"
  local ASSET_NAME="$BIN_NAME-$TARGET"
  local URL="https://github.com/$REPO/releases/download/$LATEST/$ASSET_NAME"
  local CHECKSUMS_URL="https://github.com/$REPO/releases/download/$LATEST/SHA256SUMS"
  echo "Downloading $LATEST for $TARGET..."
  mkdir -p "$INSTALL_DIR"
  (
    local TMP_BINARY="" TMP_CHECKSUMS="" EXPECTED ACTUAL
    cleanup_download() { rm -f "$TMP_BINARY" "$TMP_CHECKSUMS"; }
    trap cleanup_download EXIT HUP INT TERM

    TMP_BINARY=$(mktemp "$INSTALL_DIR/.$BIN_NAME.download.XXXXXX")
    TMP_CHECKSUMS=$(mktemp "$INSTALL_DIR/.SHA256SUMS.download.XXXXXX")
    curl -fsSL "$URL" -o "$TMP_BINARY"
    curl -fsSL "$CHECKSUMS_URL" -o "$TMP_CHECKSUMS"

    EXPECTED=$(awk -v asset="$ASSET_NAME" '
      $2 == asset {
        count++
        if (NF == 2 && length($1) == 64 && $1 ~ /^[[:xdigit:]]+$/) {
          digest = tolower($1)
        } else {
          invalid = 1
        }
      }
      END {
        if (count == 1 && !invalid) print digest
        else exit 1
      }
    ' "$TMP_CHECKSUMS") || {
      echo "Error: SHA256SUMS has no single valid entry for $ASSET_NAME" >&2
      exit 1
    }

    if command -v shasum >/dev/null 2>&1; then
      ACTUAL=$(shasum -a 256 "$TMP_BINARY" | awk '{print tolower($1)}')
    elif command -v sha256sum >/dev/null 2>&1; then
      ACTUAL=$(sha256sum "$TMP_BINARY" | awk '{print tolower($1)}')
    else
      echo "Error: shasum or sha256sum is required to verify downloads" >&2
      exit 1
    fi
    if [ "$ACTUAL" != "$EXPECTED" ]; then
      echo "Error: SHA-256 mismatch for $ASSET_NAME" >&2
      exit 1
    fi

    chmod +x "$TMP_BINARY"
    # macOS: 移除 Gatekeeper 隔离标记
    if [ "$OS" = "darwin" ]; then
      xattr -d com.apple.quarantine "$TMP_BINARY" 2>/dev/null || true
    fi
    mv -f "$TMP_BINARY" "$INSTALL_DIR/$BIN_NAME"
    echo "Installed to $INSTALL_DIR/$BIN_NAME"
  )
}

# ── 命令解析 ──

ACTION="${1:-install}"

case "$ACTION" in
  install|upgrade)
    do_install
    echo ""
    echo "Done! Restart Claude Code to see the new status line."
    exit 0
    ;;
  mini|standard)
    echo "Note: the 'mini'/'standard' subcommands are removed in this version."
    echo "      claude-lifeline now ships with the mini layout only — running plain install."
    do_install
    exit 0
    ;;
  uninstall)
    echo "Uninstalling claude-lifeline..."
    [ -f "$SETTINGS" ] && settings_remove
    if [ -f "$INSTALL_DIR/$BIN_NAME" ]; then
      rm -f "$INSTALL_DIR/$BIN_NAME"
      echo "Removed $INSTALL_DIR/$BIN_NAME"
    fi
    echo "Done! Restart Claude Code to apply."
    exit 0
    ;;
  dev)
    # 本地源码构建 + 部署，供开发者验证未发布改动
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    if [ ! -f "$SCRIPT_DIR/Cargo.toml" ]; then
      echo "Error: dev mode must be run from the repo root (Cargo.toml not found)"
      exit 1
    fi
    command -v cargo >/dev/null 2>&1 || { echo "Error: cargo not found in PATH"; exit 1; }

    echo "Building release binary from source..."
    (cd "$SCRIPT_DIR" && cargo build --release)

    BUILT="$SCRIPT_DIR/target/release/$BIN_NAME"
    [ -x "$BUILT" ] || { echo "Error: build output missing: $BUILT"; exit 1; }

    mkdir -p "$INSTALL_DIR"
    cp "$BUILT" "$INSTALL_DIR/$BIN_NAME"
    chmod +x "$INSTALL_DIR/$BIN_NAME"

    if [ "$(uname -s)" = "Darwin" ]; then
      xattr -d com.apple.quarantine "$INSTALL_DIR/$BIN_NAME" 2>/dev/null || true
    fi

    echo "Installed dev build to $INSTALL_DIR/$BIN_NAME ($("$INSTALL_DIR/$BIN_NAME" --version 2>/dev/null || echo unknown))"

    if [ -f "$SETTINGS" ]; then
      if settings_has; then
        echo "settings.json already configured"
      else
        settings_add
      fi
    else
      mkdir -p "$(dirname "$SETTINGS")"
      printf '{\n  "statusLine": {"type": "command", "command": "%s", "refreshInterval": %d}\n}\n' \
        "$STATUS_LINE_CMD" "$DEFAULT_REFRESH_INTERVAL" > "$SETTINGS"
      echo "Created $SETTINGS"
    fi

    echo ""
    echo "Done! Restart Claude Code to see the dev build."
    exit 0
    ;;
  *)
    echo "Usage: $0 [install|upgrade|uninstall|dev]"
    exit 1
    ;;
esac
