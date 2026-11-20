#!/usr/bin/env bash
set -euo pipefail

CMD="${1:-help}"
TARGET="${2:-}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${WORKSPACE_ROOT:-$(cd -- "$SCRIPT_DIR/../.." && pwd)}"
MANIFEST="$ROOT/ay/xtask/Cargo.toml"

if command -v cargo >/dev/null 2>&1 && [[ -f "$MANIFEST" ]]; then
  if [[ -n "$TARGET" ]]; then
    exec cargo run --quiet --manifest-path "$MANIFEST" -- "$CMD" "$TARGET"
  else
    exec cargo run --quiet --manifest-path "$MANIFEST" -- "$CMD"
  fi
fi

case "$CMD" in
  status)
    for r in aphrody ay fw nie rosegriffon shenron wiki; do
      dir="$ROOT/$r"
      if [[ ! -d "$dir/.git" ]]; then
        printf '%-14s | missing\n' "$r"
        continue
      fi
      branch="$(git -C "$dir" branch --show-current 2>/dev/null || echo detached)"
      dirty="$(git -C "$dir" status --short 2>/dev/null | wc -l | tr -d ' ')"
      printf '%-14s | branch=%-10s | dirty=%s\n' "$r" "$branch" "$dirty"
    done
    ;;
  tools)
    for t in bun rustc cargo sccache pwsh dotnet go python clang cmake ninja gh; do
      if command -v "$t" >/dev/null 2>&1; then
        ver="$("$t" --version 2>/dev/null | head -n 1 || true)"
        printf '%-10s | %s\n' "$t" "$ver"
      else
        printf '%-10s | NOT FOUND\n' "$t"
      fi
    done
    ;;
  cache)
    for k in CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR SCCACHE_DIR BUN_INSTALL VCPKG_DEFAULT_BINARY_CACHE; do
      printf '%-28s = %s\n' "$k" "${!k:-(unset)}"
    done
    ;;
  *)
    echo "Usage: ./scripts/xtask.sh <status|tools|cache|check> [repo]"
    ;;
esac
