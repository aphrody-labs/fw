# wgpu merge, vendor/wgpu submodule, bun:wgpu (queued, nothing pushed)

## Goal
1. Merge gfx-rs/wgpu trunk and the best open upstream PRs into aphrody-labs/wgpu `main`, ledger in `aphrody/upstream-ledger.json`, new tag `v30.0.0-aphrody.<date>`, bump the aphrody pin (`$WORKSPACE_ROOT/aphrody/Cargo.toml`, tag `v30.0.0-aphrody.20261120`, lines ~1312, 1534, 1734-1739, plus Cargo.lock wgpu entries) through a temporary index on origin/main.
2. aphrody-labs/bun: submodule `vendor/wgpu` (URL `../wgpu.git`) plus a complete `bun:wgpu` module (Rust crates, JS surface, types, docs, tests: triangle readback + vector add), built and tested on fr1 with lavapipe, optionally the triangle on the PC RTX 4070.
3. Report: upstream commit, PRs merged/dropped, tag, pin commit, Bun commits, pixel output, bytes added to the Bun binary.

## Done (all verified facts; no SHA is on origin, nothing was pushed)
- Live reads: latest wgpu release `wgpu-v30.0.2` (2026-11-12); trunk head `0432ab7fb70b28cfbfbd77e0d4b2b0c43bc49f70` (2026-11-20); fork `main` was behind trunk by 5 commits.
- fr1 `/srv/xbuild/bun-wgpu/wgpu`: clone of aphrody-labs/wgpu on `main` with a LOCAL merge commit `add0823c4` (trunk 0432ab7fb merged, clean, not pushed). Baseline on it: `cargo check --workspace --locked` rc=0 in 211 s; `cargo nextest run -p wgpu-core -p naga`: 480/483 passed, the 3 `convert_snapshots_*` failures were a missing `spirv-cross`; after `apt install spirv-cross` they pass (wgsl 17 s, glsl, spv).
- PR candidates chosen from `gh pr list -R gfx-rs/wgpu --draft=false` (68 open non-draft; 37 mergeable; CI read from check-runs): 11040 11039 10980 10959 10957 10901 10880 10862 10839 10821 10992 10583 10288 10676 10609 10873 10990 11034 11038 10289 10800 (10800 had queued CI: recheck). Excluded: CHANGES_REQUESTED (11030, 10515, 9940), failing CI (10292, 10515, 10694, 10778, 10866, 10890, 10985 which also targets the v30 branch), 11000 (CI not run), niche (10979, 10629, 10775), refactor-only 10600. No PR was merged into the fork: two gate runs were killed (sccache wrapper failures under load, then my own pkill), PR 11040 got a false `dropped` entry that was deleted with the ledger.
- Bun fork working tree (uncommitted, not compiled): `/srv/xbuild/bun-wgpu/bun` (clone of aphrody-labs/bun at origin/main 476dad4f68, which has no bun:linux/nginx/yolo code, so the model was `bun:sqlite` registration + `Bun.Image` bindgen style):
  - submodule `.gitmodules` + gitlink `vendor/wgpu` (currently checked out at upstream trunk, must be repointed to the new fork tag commit); `scripts/build/deps/wgpu.ts` (`source.kind: "local"`, `build: none`, same mechanism as lolhtml/rust-argon2 but submodule), registered in `deps/index.ts`, `source.ts` DepName, `bun.ts` (resolveDep before emitRust, vendorStamps).
  - `Cargo.toml`: members `src/wgpu`, `src/wgpu_jsc`; workspace deps `wgpu`/`naga` (path `vendor/wgpu/...`, features std, wgsl, serde / wgsl-in), `bun_wgpu`, `bun_wgpu_jsc`; `src/runtime/Cargo.toml` + `src/runtime/lib.rs` (`bun_wgpu_jsc as _`); `scripts/build/rust.ts` generatedFilesReadBy.
  - `src/wgpu` (`bun_wgpu`, no JS types: handle table, JSON descriptors, command replay, naga diagnostics, tests.rs with compute and triangle tests, per-OS backends vulkan/dx12/metal); `src/wgpu_jsc` (bindgen `wgpu.bind.ts` namespace + `wgpu_native.rs`); `src/js/bun/wgpu.ts` (WebGPU IDL classes); registration in `HardcodedModule.rs` (3 places), `isBuiltinModule.cpp`, `src/js/private.d.ts`, `src/codegen/builtins/data/module-exports.txt`; `packages/bun-types/wgpu.d.ts` (NOT yet referenced from `index.d.ts`); `docs/runtime/wgpu.mdx` (NOT yet in `docs/docs.json`); `test/js/bun/wgpu/wgpu.test.ts`.
  - Same files are copied in `$WORKSPACE_ROOT/_work/bunwgpu/` on the PC (source of truth for the new files).
- Findings that will bite: `cargo check -p bun_wgpu` failed in `wgpu-sync` because Bun's Cargo.lock resolved `lock_api` 0.5.0 while wgpu-sync needs the 0.4 API (the fork's own lock pins 0.4.14); `cargo update -p lock_api --precise 0.4.14` was attempted, Cargo.lock still shows 0.5.0, so the pin failed and must be solved (pin via a different route, or tighten `lock_api` to `<0.5` in the fork's workspace Cargo.toml as part of the fork commit). The rust code of `src/wgpu` has never been compiled: expect wgpu v30 signature fixes (`InstanceDescriptor::with_env`, `Features::iter_names`/`as_str`, `PipelineCompilationOptions.constants`, `MipmapFilterMode`, `DeviceDescriptor`, `RenderBundleEncoderDescriptor`, `device.set_device_lost_callback`). The Bun build workspace has `warnings = deny`, `unreachable_pub`, `dead_code` deny.

## Remaining
1. fr1 wgpu loop: fix and rerun the PR loop (`handoff/wgpu-prs.sh`, `wgpu-gate.sh`; gate = `cargo check --workspace --locked` per PR, nextest of wgpu-core+naga when the PR touches naga, wgpu-core, wgpu-types or wgpu-naga-bridge). Use `RUSTC_WRAPPER=` (sccache fails under load) and never `pkill -f` a pattern that appears in your own ssh command. Write `aphrody/upstream-ledger.json` with `handoff/ledger.ts`. Fetch/merge origin/main before push; tag `v30.0.0-aphrody.20261121`.
2. Aphrody pin bump commit via temporary index (the local checkout has other sessions' edits).
3. Bun: repoint the submodule to the new fork commit; make `cargo test -p bun_wgpu` pass on fr1 (lavapipe at `/usr/share/vulkan/icd.d/lvp_icd.json` is installed); then full build with `CARGO_PROFILE_DEV_DEBUG=line-tables-only` (cuts rust-target from 13 GB; plan.ts passes process env to cargo) and `bun bd test test/js/bun/wgpu/wgpu.test.ts`; fix the codegen checks (module-exports, bindgen types); add the d.ts reference and the docs.json entry; measure the binary size delta (build with and without the crates) in bytes; push only after the build is green (fetch and merge origin/main first; another worker is merging the canary into aphrody-labs/bun).
4. Optional: triangle on the RTX 4070 by running `bun_wgpu`'s `tests.rs` in a scratch workspace on the PC.

## Resume
```
ssh fr1
cd /srv/xbuild/bun-wgpu/bun      # Bun fork tree (uncommitted)
cd /srv/xbuild/bun-wgpu/wgpu     # wgpu fork clone, branch main, remote upstream = gfx-rs/wgpu
RUSTC_WRAPPER= CARGO_TARGET_DIR=/srv/xbuild/bun-wgpu/target-bun RUSTUP_TOOLCHAIN=nightly-2026-11-14 cargo test -p bun_wgpu
cd /srv/xbuild/bun-wgpu/wgpu && CARGO_TARGET_DIR=/srv/xbuild/bun-wgpu/target-wgpu ~/ci/wgpu-prs.sh 11040 11039 ...
```
Note `~/ci/wgpu-gate.sh` exports `CARGO_TARGET_DIR=$HOME/ci/wgpu/target`, which resolves through the symlink to `/srv/xbuild`, so nothing lands on `/`.

## Directories and files created
fr1:
- `/srv/xbuild/bun-wgpu/bun` 2.1 GB (Bun clone, uncommitted work; `build/` 686 MB inside is a partial debug build, safe to delete)
- `/srv/xbuild/bun-wgpu/wgpu` 75 MB (wgpu clone with the local trunk merge; keep until pushed)
- `/srv/xbuild/bun-wgpu/handoff` 76 KB (scripts and copies of the new files)
- symlinks `~/ci/bun-wgpu`, `~/ci/wgpu` to the two clones; logs `~/ci/bun-wgpu-build0.log` 72 KB, `~/ci/bwgpu-check.log` 16 KB, `~/ci/wgpu-base.log`, `~/ci/wgpu-prs.log`, `~/ci/wgpu-gate.sh`, `~/ci/wgpu-prs.sh` (all safe to delete)
- `/dev/shm/bwgpu-target` was removed; no process of mine is running.
- Outside my scope but done: removed the stale `~/target-ci` (25 GB, last touched 14 h earlier) and old `~/src/fw/target/debug/incremental` entries; `sudo apt-get install spirv-cross`.
PC: `$WORKSPACE_ROOT/_work/bunwgpu` (a few hundred KB of source; safe to delete once the work is committed).
prd: nothing.
