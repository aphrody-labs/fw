#!/usr/bin/env sh
# Zero-manual installer, updater & uninstaller for fw (Linux & macOS).
# Install / Update: curl -fsSL https://fw-edge.aphrody-labs.workers.dev/install.sh | sh
# Uninstall:        curl -fsSL https://fw-edge.aphrody-labs.workers.dev/install.sh | sh -s -- --uninstall
set -eu

REPO="aphrody-labs/fw"
BIN_DIR="${FW_INSTALL_DIR:-$HOME/.local/bin}"
ACTION="install"
VERSION=""

while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) ACTION="uninstall"; shift ;;
    --version) VERSION="${2:-}"; shift 2 ;;
    *) shift ;;
  esac
done

if [ "$ACTION" = "uninstall" ]; then
  rm -f "$BIN_DIR/fw" "$BIN_DIR/fw.prev" "$BIN_DIR/fw.new"
  echo "fw uninstalled from $BIN_DIR"
  exit 0
fi

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
  linux-x86_64)  ASSET="fw-x86_64-linux-musl" ;;
  darwin-arm64|darwin-aarch64) ASSET="fw-aarch64-macos" ;;
  *) echo "Unsupported platform: $OS-$ARCH" >&2; exit 1 ;;
esac

if [ -n "$VERSION" ]; then
  URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"
else
  URL="https://github.com/$REPO/releases/latest/download/$ASSET"
fi

mkdir -p "$BIN_DIR"
TMP="$BIN_DIR/fw.new"
curl -fsSL "$URL" -o "$TMP"
chmod +x "$TMP"
[ -f "$BIN_DIR/fw" ] && mv -f "$BIN_DIR/fw" "$BIN_DIR/fw.prev" || true
mv -f "$TMP" "$BIN_DIR/fw"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
      if [ -f "$rc" ] && ! grep -q "$BIN_DIR" "$rc" 2>/dev/null; then
        printf '\nexport PATH="%s:$PATH"\n' "$BIN_DIR" >> "$rc"
      fi
    done
    ;;
esac

"$BIN_DIR/fw" --version
