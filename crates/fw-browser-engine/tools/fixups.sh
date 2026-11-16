#!/usr/bin/env bash
# Integration fixes on top of flatten.py output; idempotent, rerun after every re-flatten.
set -e
cd "$(dirname "$0")/.."
# drop a package from the members list and from [patch.crates-io]: drop <dir> <package>
drop() { sed -i "s#\"src/$1\", ##; s#, \"src/$1\"##; \#^$2 = {path = \"src/$1\"}#d" Cargo.toml; }

# surfman main dropped the sm-angle-default feature; ANGLE built from mozangle is sm-angle-builtin
rg -l "sm-angle-default" src --glob Cargo.toml | xargs -r sed -i "s/sm-angle-default/sm-angle-builtin/g"

# dev tools: wrench pulls a second fontconfig-sys (links = "fontconfig"), the examples are demos
drop wrench wrench
drop webrender-examples webrender-examples

# The Obscura script layer is deno_core -> rusty_v8 (published V8 prebuilts), which cannot resolve next to
# mozjs_sys (icu_capi =2.1.2 vs temporal_capi icu ^2.2) and is replaced by our own V8 bindings (src/v8-sys,
# not written yet). The crates that need it stay in src/ but out of the workspace until then.
for c in obscura-js obscura-browser obscura-cdp obscura-cli obscura obscura-mcp; do drop $c $c; done

# webrender used time 0.1's precise_time_ns, gone in time 0.3 (the version servo and the rest use)
for f in src/webrender_api/src/display_list.rs src/webrender/src/scene_builder_thread.rs; do
  perl -0pi -e 's/use time::precise_time_ns;/fn precise_time_ns() -> u64 { static S: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new(); S.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64 }/' "$f"
done
