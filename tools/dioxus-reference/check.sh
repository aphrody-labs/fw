#!/bin/sh
# Builds and tests the reference app against each Dioxus version: clippy and tests on the server feature, then the wasm32 web build with dx.
# Usage: check.sh [version ...]   (default: max_stable_version and max_version of the dioxus crate from docs/research/dioxuslabs-ecosystem.json)
# Run it on a cluster node (fr1 /srv/xbuild, prd /srv/build), never on the PC. Needs cargo, jq, cargo-binstall (or dx on PATH) and the wasm32-unknown-unknown target.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
snapshot="$here/../../docs/research/dioxuslabs-ecosystem.json"
work=${DIOXUS_CHECK_DIR:-${TMPDIR:-/tmp}/dioxus-reference-check}
if [ "$#" -eq 0 ]; then
  set -- $(jq -r '.crates.dioxus | .max_stable_version, .max_version' "$snapshot" | sort -u)
fi
for version in "$@"; do
  dir="$work/$version"
  rm -rf "$dir/src" "$dir/assets"
  mkdir -p "$dir"
  cp -R "$here/src" "$here/assets" "$here/Cargo.toml" "$dir/"
  sed -i "s/^dioxus = { version = \"=[^\"]*\"/dioxus = { version = \"=$version\"/" "$dir/Cargo.toml"
  (cd "$dir"
    export CARGO_TARGET_DIR="$work/target-$version"
    cargo clippy --features server --all-targets -- -D warnings
    cargo test --features server
    [ -x "$work/dx-$version/bin/dx" ] || cargo binstall --no-confirm --root "$work/dx-$version" "dioxus-cli@$version"
    "$work/dx-$version/bin/dx" build --release --web --fullstack true
    find "$CARGO_TARGET_DIR/dx" -name '*.wasm' -exec ls -l {} +)
done
