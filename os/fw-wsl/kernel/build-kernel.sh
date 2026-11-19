#!/usr/bin/env bash
# Builds Fw Custom WSL2 Kernel (linux-msft-wsl-6.18.58.1 + CachyOS BORE/sched_ext + Clang ThinLTO)
# Run on fr1 (/srv/xbuild/fw-wsl-kernel) — never heavy-build kernels on local Windows PC!
set -euo pipefail

WORK="${1:-/srv/xbuild/fw-wsl-kernel}"
KERNEL_TAG="${KERNEL_TAG:-linux-msft-wsl-6.18.58.1}"
FRAGMENT="$(cd "$(dirname "$0")" && pwd)/fw-wsl-6.18.config"

mkdir -p "$WORK"
cd "$WORK"

if [ ! -d WSL2-Linux-Kernel ]; then
  git clone --depth 1 --branch "$KERNEL_TAG" https://github.com/microsoft/WSL2-Linux-Kernel.git
fi
if [ ! -d kernel-patches ]; then
  git clone --depth 1 https://github.com/CachyOS/kernel-patches.git
fi

cd WSL2-Linux-Kernel
# Apply CachyOS BORE scheduler patch if matching 6.18 series exists
BORE_PATCH=$(find ../kernel-patches/6.18 -name "*bore*.patch" 2>/dev/null | head -n 1 || true)
if [ -n "$BORE_PATCH" ]; then
  git apply --check "$BORE_PATCH" 2>/dev/null && git apply "$BORE_PATCH" || echo "WARN: BORE patch already applied or skipped"
fi

cp Microsoft/config-wsl .config
scripts/kconfig/merge_config.sh -m .config "$FRAGMENT"
make LLVM=1 LLVM_IAS=1 olddefconfig
make -j"$(nproc)" LLVM=1 LLVM_IAS=1 KCFLAGS="-O3 -march=x86-64-v3" bzImage

echo "BUILT: $(pwd)/arch/x86/boot/bzImage"
ls -lh arch/x86/boot/bzImage
