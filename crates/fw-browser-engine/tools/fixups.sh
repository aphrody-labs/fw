#!/usr/bin/env bash
# Integration fixes on top of flatten.py output; idempotent, rerun after every re-flatten.
set -e
cd "$(dirname "$0")/.."
# drop a package from the members list and from [patch.crates-io]: drop <dir> <package>
# crate <name>: the directory of an imported crate, third_party/<upstream>/<name> (tools/layout.toml)
crate() { printf '%s\n' third_party/*/"$1"; }
drop() { d=$(crate "$1"); sed -i "s#\"$d\", ##; s#, \"$d\"##; \#^$2 = {path = \"$d\"}#d" Cargo.toml; }

# surfman main dropped the sm-angle-default feature; ANGLE built from mozangle is sm-angle-builtin
rg -l "sm-angle-default" third_party --glob Cargo.toml | xargs -r sed -i "s/sm-angle-default/sm-angle-builtin/g"

# dev tools: wrench pulls a second fontconfig-sys (links = "fontconfig"), the examples are demos
drop wrench wrench
drop webrender-examples webrender-examples

# The Obscura script layer is deno_core -> rusty_v8 (published V8 prebuilts), which cannot resolve next to
# mozjs_sys (icu_capi =2.1.2 vs temporal_capi icu ^2.2) and is replaced by our own V8 bindings (components/v8-sys,
# not written yet). The crates that need it stay in third_party/obscura but out of the workspace until then.
for c in obscura-js obscura-browser obscura-cdp obscura-cli obscura obscura-mcp; do drop $c $c; done

# webrender used time 0.1's precise_time_ns, gone in time 0.3 (the version servo and the rest use)
for f in "$(crate webrender_api)/src/display_list.rs" "$(crate webrender)/src/scene_builder_thread.rs"; do
  perl -0pi -e 's/use time::precise_time_ns;/fn precise_time_ns() -> u64 { static S: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new(); S.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64 }/' "$f"
done

# hyperfine is edition 2018 (resolver 1): its normal and build-dependency clap share one feature set (build.rs includes
# src/cli.rs). The workspace resolver 2 keeps them apart: give the normal clap `std` and the build one `cargo`, `wrap_help`.
sed -i '/^clap = {version = "4", default-features = false/s/{version = "4", /{version = "4.6", /; /^clap = {version = "4.6", default-features = false/s/features = \["suggestions"/features = ["std", "suggestions"/; s/^clap = "4.6"$/clap = {version = "4.6", features = ["cargo", "wrap_help"]}/' "$(crate hyperfine)/Cargo.toml"
