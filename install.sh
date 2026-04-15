#!/bin/bash
set -euo pipefail

REPO="xjoker/claude-lifeline"
INSTALL_DIR="$HOME/.claude/bin"
BIN_NAME="claude-lifeline"
SETTINGS="$HOME/.claude/settings.json"
STATUS_LINE_CMD="~/.claude/bin/claude-lifeline"
STATUS_LINE_JSON='{"type":"command","command":"~/.claude/bin/claude-lifeline"}'

# ── JSON helpers (jq preferred, sed fallback) ──

has_jq() { command -v jq &>/dev/null; }

settings_add() {
  cp "$SETTINGS" "$SETTINGS.bak"
  if has_jq; then
    jq --argjson sl "$STATUS_LINE_JSON" '.statusLine = $sl' "$SETTINGS.bak" > "$SETTINGS"
  else
    # sed fallback: insert before final }
    sed -i.tmp 's/}[[:space:]]*$/,"statusLine":{"type":"command","command":"~\/.claude\/bin\/claude-lifeline"}}/' "$SETTINGS"
    rm -f "$SETTINGS.tmp"
  fi
  echo "Updated settings.json (backup: settings.json.bak)"
}

settings_remove() {
  if ! grep -q '"statusLine"' "$SETTINGS" 2>/dev/null; then
    echo "No statusLine config found in settings.json"
    return
  fi
  cp "$SETTINGS" "$SETTINGS.bak"
  if has_jq; then
    jq 'del(.statusLine)' "$SETTINGS.bak" > "$SETTINGS"
  else
    echo "Warning: jq not found. Please manually remove \"statusLine\" from $SETTINGS"
  fi
  echo "Removed statusLine from settings.json (backup: settings.json.bak)"
}

settings_has() {
  grep -q "\"command\".*\"$STATUS_LINE_CMD\"" "$SETTINGS" 2>/dev/null
}

# ── 命令解析 ──

ACTION="${1:-install}"

case "$ACTION" in
  install|upgrade) ;;
  uninstall)
    echo "Uninstalling claude-lifeline..."
    rm -f "$INSTALL_DIR/$BIN_NAME"
    [ -f "$SETTINGS" ] && settings_remove
    echo "Done! Restart Claude Code to apply."
    exit 0
    ;;
  *)
    echo "Usage: $0 [install|upgrade|uninstall]"
    exit 1
    ;;
esac

# ── 平台检测 ──

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

# ── 版本检查 ──

LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST" ]; then
  echo "Error: failed to fetch latest release"
  exit 1
fi

# --version 输出 "claude-lifeline 0.0.1", tag 是 "v0.0.1"
LATEST_VER="${LATEST#v}"

if [ "$ACTION" = "upgrade" ] && [ -x "$INSTALL_DIR/$BIN_NAME" ]; then
  CURRENT=$("$INSTALL_DIR/$BIN_NAME" --version 2>/dev/null || echo "unknown")
  echo "Current: $CURRENT, Latest: $LATEST"
  if [ "$CURRENT" = "$BIN_NAME $LATEST_VER" ]; then
    echo "Already up to date."
    exit 0
  fi
fi

# ── 下载 ──

URL="https://github.com/$REPO/releases/download/$LATEST/$BIN_NAME-$TARGET"
echo "Downloading $LATEST for $TARGET..."

mkdir -p "$INSTALL_DIR"
curl -fsSL "$URL" -o "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

# macOS: 移除 Gatekeeper 隔离标记
if [ "$OS" = "darwin" ]; then
  xattr -d com.apple.quarantine "$INSTALL_DIR/$BIN_NAME" 2>/dev/null || true
fi

echo "Installed to $INSTALL_DIR/$BIN_NAME"

# ── 配置 settings.json ──

if [ -f "$SETTINGS" ]; then
  if settings_has; then
    echo "settings.json already configured"
  else
    settings_add
  fi
else
  mkdir -p "$(dirname "$SETTINGS")"
  printf '{\n  "statusLine": {"type": "command", "command": "%s"}\n}\n' "$STATUS_LINE_CMD" > "$SETTINGS"
  echo "Created $SETTINGS"
fi

echo ""
echo "Done! Restart Claude Code to see the new status line."
