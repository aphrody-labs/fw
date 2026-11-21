#!/usr/bin/env bash
# Builds the fw android WSL2 guest kernel on the fleet (fr1) and leaves bzImage plus its config and sha256 in $OUT.
# usage: build.sh [workdir]   (default /srv/xbuild/android-win/kernel; run on fr1, never on the PC)
set -euo pipefail
TAG=linux-msft-wsl-6.18.58.1
COMMIT=a989171929bed4ea681be8d77333845ce1c1732a
here=$(cd "$(dirname "$0")" && pwd)
work=${1:-/srv/xbuild/android-win/kernel}
mkdir -p "$work"
if [ ! -d "$work/src/.git" ]; then
  git clone --depth 1 --branch "$TAG" https://github.com/microsoft/WSL2-Linux-Kernel.git "$work/src"
fi
cd "$work/src"
[ "$(git rev-parse HEAD)" = "$COMMIT" ] || { echo "kernel checkout is not $COMMIT" >&2; exit 1; }
cp Microsoft/config-wsl .config
scripts/kconfig/merge_config.sh -m .config "$here/fw-android.config" >/dev/null
make olddefconfig
for option in ANDROID_BINDER_IPC ANDROID_BINDERFS; do
  grep -qx "CONFIG_$option=y" .config || { echo "CONFIG_$option did not survive olddefconfig" >&2; exit 1; }
done
make -j"$(nproc)" bzImage
mkdir -p "$work/out"
cp arch/x86/boot/bzImage "$work/out/bzImage"
cp .config "$work/out/config"
(cd "$work/out" && sha256sum bzImage > bzImage.sha256 && ls -l bzImage && cat bzImage.sha256)
