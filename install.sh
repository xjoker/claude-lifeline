#!/bin/bash
set -euo pipefail

REPO="xjoker/claude-lifeline"
INSTALL_DIR="$HOME/.claude/bin"
BIN_NAME="claude-lifeline"
SETTINGS="$HOME/.claude/settings.json"

# ── 平台检测 ──

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS-$ARCH" in
  darwin-arm64)    TARGET="aarch64-apple-darwin" ;;
  darwin-x86_64)   TARGET="x86_64-apple-darwin" ;;
  linux-x86_64)    TARGET="x86_64-unknown-linux-gnu" ;;
  linux-aarch64)   TARGET="aarch64-unknown-linux-gnu" ;;
  *)
    echo "Error: unsupported platform $OS-$ARCH"
    exit 1
    ;;
esac

echo "Platform: $OS/$ARCH -> $TARGET"

# ── 下载 ──

LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST" ]; then
  echo "Error: failed to fetch latest release"
  exit 1
fi

URL="https://github.com/$REPO/releases/download/$LATEST/$BIN_NAME-$TARGET"
echo "Downloading $LATEST for $TARGET..."

mkdir -p "$INSTALL_DIR"
curl -fsSL "$URL" -o "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

echo "Installed to $INSTALL_DIR/$BIN_NAME"

# ── 配置 settings.json ──

if [ -f "$SETTINGS" ]; then
  if command -v python3 &>/dev/null; then
    python3 - "$SETTINGS" <<'PYEOF'
import json, sys, shutil

path = sys.argv[1]
with open(path) as f:
    d = json.load(f)

current = d.get("statusLine", {}).get("command", "")
if current == "~/.claude/bin/claude-lifeline":
    print("settings.json already configured")
else:
    shutil.copy(path, path + ".bak")
    d["statusLine"] = {"type": "command", "command": "~/.claude/bin/claude-lifeline"}
    with open(path, "w") as f:
        json.dump(d, f, indent=2, ensure_ascii=False)
        f.write("\n")
    print("Updated settings.json (backup: settings.json.bak)")
PYEOF
  else
    echo "Warning: python3 not found, please manually add to ~/.claude/settings.json:"
    echo '  "statusLine": { "type": "command", "command": "~/.claude/bin/claude-lifeline" }'
  fi
else
  echo "Warning: $SETTINGS not found. Create it or add statusLine config manually."
fi

echo ""
echo "Done! Restart Claude Code to see the new status line."
