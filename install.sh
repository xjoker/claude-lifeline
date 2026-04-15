#!/bin/bash
set -euo pipefail

REPO="xjoker/claude-lifeline"
INSTALL_DIR="$HOME/.claude/bin"
BIN_NAME="claude-lifeline"
SETTINGS="$HOME/.claude/settings.json"
STATUS_LINE_CMD="~/.claude/bin/claude-lifeline"

# ── JSON helpers (no python3 / jq dependency) ──

# Check if settings.json contains statusLine command
has_status_line() {
  grep -q "\"command\".*\"$STATUS_LINE_CMD\"" "$SETTINGS" 2>/dev/null
}

# Add statusLine to settings.json using sed
add_status_line() {
  cp "$SETTINGS" "$SETTINGS.bak"
  # Remove existing statusLine block if present
  if grep -q '"statusLine"' "$SETTINGS"; then
    # Replace existing statusLine object (handles multi-line)
    local tmp
    tmp=$(mktemp)
    awk '
      /"statusLine"/ { skip=1; brace=0 }
      skip && /{/ { brace++ }
      skip && /}/ { brace--; if(brace<=0){skip=0; next} }
      skip { next }
      { print }
    ' "$SETTINGS" > "$tmp"
    mv "$tmp" "$SETTINGS"
  fi
  # Insert statusLine before the closing brace
  local tmp
  tmp=$(mktemp)
  awk '
    /^}[[:space:]]*$/ {
      # Remove trailing comma issues - add comma to previous non-empty line
      print "  ,\"statusLine\": {\"type\": \"command\", \"command\": \"'"$STATUS_LINE_CMD"'\"}"
    }
    { print }
  ' "$SETTINGS" > "$tmp"
  mv "$tmp" "$SETTINGS"
  echo "Updated settings.json (backup: settings.json.bak)"
}

# Remove statusLine from settings.json
remove_status_line() {
  if ! grep -q '"statusLine"' "$SETTINGS"; then
    echo "No statusLine config found in settings.json"
    return
  fi
  cp "$SETTINGS" "$SETTINGS.bak"
  local tmp
  tmp=$(mktemp)
  awk '
    /"statusLine"/ { skip=1; brace=0; comma_before=prev_comma }
    skip && /{/ { brace++ }
    skip && /}/ { brace--; if(brace<=0){skip=0; next} }
    skip { next }
    {
      prev_comma = /,$/
      print
    }
  ' "$SETTINGS" > "$tmp"
  mv "$tmp" "$SETTINGS"
  echo "Removed statusLine from settings.json (backup: settings.json.bak)"
}

# ── 命令解析 ──

ACTION="${1:-install}"

case "$ACTION" in
  install|upgrade) ;;
  uninstall)
    echo "Uninstalling claude-lifeline..."
    rm -f "$INSTALL_DIR/$BIN_NAME"
    if [ -f "$SETTINGS" ]; then
      remove_status_line
    fi
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

if [ "$ACTION" = "upgrade" ] && [ -x "$INSTALL_DIR/$BIN_NAME" ]; then
  CURRENT=$("$INSTALL_DIR/$BIN_NAME" --version 2>/dev/null || echo "unknown")
  echo "Current: $CURRENT, Latest: $LATEST"
  if [ "$CURRENT" = "$BIN_NAME $LATEST" ] || [ "$CURRENT" = "$LATEST" ]; then
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

echo "Installed to $INSTALL_DIR/$BIN_NAME"

# ── 配置 settings.json ──

if [ -f "$SETTINGS" ]; then
  if has_status_line; then
    echo "settings.json already configured"
  else
    add_status_line
  fi
else
  # Create minimal settings.json
  mkdir -p "$(dirname "$SETTINGS")"
  echo '{"statusLine": {"type": "command", "command": "'"$STATUS_LINE_CMD"'"}}' > "$SETTINGS"
  echo "Created $SETTINGS"
fi

echo ""
echo "Done! Restart Claude Code to see the new status line."
