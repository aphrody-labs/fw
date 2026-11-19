#!/usr/bin/env bash
# Reproducible full-stack Rust userland Nix bootstrapper for all 3 VPS (fr1, fr2, prd).
# Works identically on Ubuntu 26.10 (fr1/fr2) and Arch Linux (prd) with zero drift.
#
# Usage:
#   ./nix/bootstrap-vps.sh <fr1|fr2|prd> [flake-ref]
set -euo pipefail

ROLE="${1:-$(hostname -s)}"
FLAKE_REF="${2:-github:aphrody-labs/fw}"

echo "[fw-nix] Bootstrapping role=${ROLE} flake=${FLAKE_REF}"

# 1. Install multi-user Nix with flakes if absent
if ! command -v nix >/dev/null 2>&1 && [ ! -x /nix/var/nix/profiles/default/bin/nix ]; then
  curl -sSfL https://install.determinate.systems/nix | sh -s -- install --no-confirm
fi

if [ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]; then
  # shellcheck disable=SC1091
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
fi

# 2. Ensure flakes + store deduplication are enabled
sudo mkdir -p /etc/nix
if ! grep -q "experimental-features" /etc/nix/nix.conf 2>/dev/null; then
  printf '\nexperimental-features = nix-command flakes\nauto-optimise-store = true\n' | sudo tee -a /etc/nix/nix.conf >/dev/null
fi

# 3. Install or upgrade the exact identical full-stack pure-Rust userland closure
if nix profile list 2>/dev/null | grep -q "fw-rust-userland"; then
  nix profile upgrade --all
else
  nix profile install "${FLAKE_REF}#rust-userland"
fi

# 4. Verify core pure-Rust userland binaries from /nix/store
for bin in fw-native nu rg fd ast-grep jaq xh sd bat eza jj difft just sccache mold; do
  command -v "$bin" >/dev/null 2>&1 && echo "  OK: $(command -v "$bin")" || echo "  WARN: missing $bin"
done

echo "[fw-nix] Done for ${ROLE}."
