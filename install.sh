#!/bin/bash
set -euo pipefail

REPO="xjoker/claude-lifeline"
INSTALL_DIR="$HOME/.claude/bin"
BIN_NAME="claude-lifeline"
SETTINGS="$HOME/.claude/settings.json"
CONFIG="$HOME/.claude/claude-lifeline/config.toml"
STATUS_LINE_CMD="~/.claude/bin/claude-lifeline"
STATUS_LINE_JSON='{"type":"command","command":"~/.claude/bin/claude-lifeline"}'

# ── Layout config helpers (~/.claude/claude-lifeline/config.toml) ──

# 设置 [display] layout 为指定值（mini | auto | single | multi），保留其他配置
set_layout() {
  local layout="$1"
  mkdir -p "$(dirname "$CONFIG")"
  if [ ! -f "$CONFIG" ]; then
    printf '[display]\nlayout = "%s"\n' "$layout" > "$CONFIG"
    echo "Created $CONFIG with layout = \"$layout\""
    return
  fi
  cp "$CONFIG" "$CONFIG.bak"
  # 用 awk 做段内替换/插入，避免误改 [display] 之外 segment 里同名字段
  awk -v layout="$layout" '
    BEGIN { in_display = 0; replaced = 0 }
    # 段标题
    /^\[[^]]+\][[:space:]]*$/ {
      # 离开 [display] 前，如还没替换成功，就在段末尾补一行
      if (in_display && !replaced) {
        print "layout = \"" layout "\""
        replaced = 1
      }
      in_display = ($0 ~ /^\[display\][[:space:]]*$/)
      print
      next
    }
    # [display] 段内的 layout 行 → 替换
    in_display && /^[[:space:]]*layout[[:space:]]*=/ {
      print "layout = \"" layout "\""
      replaced = 1
      next
    }
    { print }
    END {
      if (in_display && !replaced) {
        print "layout = \"" layout "\""
        replaced = 1
      }
      # 文件完全没有 [display] 段 → 追加新段
      if (!replaced) {
        print ""
        print "[display]"
        print "layout = \"" layout "\""
      }
    }
  ' "$CONFIG.bak" > "$CONFIG"
  echo "Set layout = \"$layout\" in $CONFIG (backup: config.toml.bak)"
}

# ── JSON helpers (jq preferred, sed fallback) ──

has_jq() { command -v jq &>/dev/null; }

settings_add() {
  cp "$SETTINGS" "$SETTINGS.bak"
  if has_jq; then
    jq --argjson sl "$STATUS_LINE_JSON" '.statusLine = $sl' "$SETTINGS.bak" > "$SETTINGS"
  else
    # sed fallback: 区分空对象 {} 与已有键的情况
    #   空对象：`{,"statusLine":...}` 会是无效 JSON，需要不带逗号的形式
    #   有键：在最后 } 前插入 `,"statusLine":...`
    if grep -q '"' "$SETTINGS"; then
      sed -i.tmp 's/}[[:space:]]*$/,"statusLine":{"type":"command","command":"~\/.claude\/bin\/claude-lifeline"}}/' "$SETTINGS"
    else
      printf '{"statusLine":{"type":"command","command":"%s"}}\n' "$STATUS_LINE_CMD" > "$SETTINGS"
    fi
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
    printf '{\n  "statusLine": {"type": "command", "command": "%s"}\n}\n' "$STATUS_LINE_CMD" > "$SETTINGS"
    echo "Created $SETTINGS"
  fi
}

_download_binary() {
  local LATEST="$1" TARGET="$2" OS="$3"
  local URL="https://github.com/$REPO/releases/download/$LATEST/$BIN_NAME-$TARGET"
  echo "Downloading $LATEST for $TARGET..."
  mkdir -p "$INSTALL_DIR"
  curl -fsSL "$URL" -o "$INSTALL_DIR/$BIN_NAME"
  chmod +x "$INSTALL_DIR/$BIN_NAME"
  # macOS: 移除 Gatekeeper 隔离标记
  if [ "$OS" = "darwin" ]; then
    xattr -d com.apple.quarantine "$INSTALL_DIR/$BIN_NAME" 2>/dev/null || true
  fi
  echo "Installed to $INSTALL_DIR/$BIN_NAME"
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
  mini)
    do_install
    set_layout "mini"
    echo ""
    echo "Done! Restart Claude Code to apply mini layout."
    exit 0
    ;;
  standard)
    do_install
    set_layout "auto"
    echo ""
    echo "Done! Restart Claude Code to apply standard layout."
    exit 0
    ;;
  uninstall)
    echo "Uninstalling claude-lifeline..."
    rm -f "$INSTALL_DIR/$BIN_NAME"
    [ -f "$SETTINGS" ] && settings_remove
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
      printf '{\n  "statusLine": {"type": "command", "command": "%s"}\n}\n' "$STATUS_LINE_CMD" > "$SETTINGS"
      echo "Created $SETTINGS"
    fi

    echo ""
    echo "Done! Restart Claude Code to see the dev build."
    exit 0
    ;;
  *)
    echo "Usage: $0 [install|upgrade|uninstall|dev|mini|standard]"
    exit 1
    ;;
esac
