#!/usr/bin/env bash
# Build the engines from a Chromium checkout. One variable switches the generation:
#   CHROMIUM_SRC=<the Chromium checkout>     (required; its revision is DEPS chromium_version)
#   OUT=out/browser  TARGET=browser:engines   (browser:blink = headless_shell, native CDP)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
src=${CHROMIUM_SRC:?set CHROMIUM_SRC to the Chromium checkout (private: .local/config.toml [paths] chromium_src)}
out=${OUT:-out/browser}
target=${TARGET:-browser:engines}
export PATH="$src/buildtools/linux64:$PATH"
ln -sfn "$here" "$src/browser"
cd "$src"
mkdir -p "$out" && cp "$here/args.gn" "$out/args.gn"
gn gen "$out" --root-target="//$target"
ninja -C "$out" "$target"
