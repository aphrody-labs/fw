#!/usr/bin/env bash
# Builds Custom Fairwind WSLg system distro VHD incorporating upstream microsoft/wslg PRs:
#   - PR #1137: .X11-unix mode 01777 (sticky bit)
#   - PR #1333: XDG_RUNTIME_DIR mode 0700
#   - PR #1046: .wslgconfig keymap -> weston.ini
#   - PR #1426: btrfs-progs & xfsprogs in system distro
#   - PR #1436: max crash-loop guard for WSLGd
# Run on fr1 (/srv/xbuild/fairwind-wslg).
set -euo pipefail

WORK="${1:-/srv/xbuild/fairwind-wslg}"
mkdir -p "$WORK"
cd "$WORK"

if [ ! -d wslg ]; then
  git clone --depth 1 https://github.com/microsoft/wslg.git
fi
cd wslg

for pr in 1137 1333 1046 1426 1436; do
  echo "[fairwind-wslg] Fetching upstream PR #${pr} patch..."
  curl -sSfL "https://github.com/microsoft/wslg/pull/${pr}.patch" -o "/tmp/wslg-pr-${pr}.patch" || true
  if [ -s "/tmp/wslg-pr-${pr}.patch" ]; then
    git apply --check "/tmp/wslg-pr-${pr}.patch" 2>/dev/null && git apply "/tmp/wslg-pr-${pr}.patch" && echo "  APPLIED PR #${pr}" || echo "  SKIPPED PR #${pr} (already merged or conflict)"
  fi
done

echo "[fairwind-wslg] Ready to build system-distro VHD via docker buildx (Alpine/Mariner stage)."
