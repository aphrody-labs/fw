# wasm-toolchain-pin-optimize

## Goal
Pin the whole WebAssembly toolchain with exact versions and sha256 plus an installer that fails on drift (fr1 and PC), then optimize the aphrody web desktop wasm (raw, brotli, gzip per cohort, first frame in Chrome on the RTX 4070) from twiggy and wasm-opt numbers, and document the result in the `wasm-bindgen` and `low-level-lto-aot` skills.

## Done
- Pins + installer: `configs/claude/toolchain.toml` (group `wasm`), `configs/claude/scripts/toolchain.ts` (curl download with sha256, `tar.xz`, `dir` SDK installs, `cargo` source builds, `post`, `expect`, `--group`, `lock` drift checks against `aphrody/Cargo.lock` and `BINARYEN_VERSION` in `aphrody-ci.yml`), tests in `toolchain.test.ts` (8 pass).
- Pinned: wasm-bindgen + wasm-bindgen-test-runner 0.2.130, Binaryen 133 (wasm-opt, wasm-merge, wasm-metadce, wasm-split, wasm-dis, wasm-as), wasm-tools 1.263.0, WABT 1.0.42 (wasm2wat, wat2wasm, wasm-objdump, wasm-strip, wasm-validate), wit-bindgen 0.63.0, wasmtime 51.0.0, wasmer 7.5.0, twiggy 0.8.0 (crate source), wasi-sdk 34.0, emsdk 7.0.0 (fleet only), cargo-component 0.21.1 (fleet only).
- PC: `bun configs/claude/scripts/toolchain.ts check --group wasm` reports every tool ok (wasmer, twiggy, wasi-sdk installed by it).
- Skills: pins and the measured wasm-opt table added to `plugins/aphrody/skills/wasm-bindgen/SKILL.md`; one line in `low-level-lto-aot/SKILL.md`; 0.2.129 corrected to 0.2.130.
- Verified on fr1 before the stop: wasm-opt 133, wasm-tools, wabt, wasmtime 51, wasmer 7.5, wit-bindgen on PATH from `~/.local/bin`; the fr1 `install --group wasm` run was killed while building twiggy/cargo-component, so fr1 still lacks: twiggy, cargo-component, wasi-sdk, emsdk (wasm-bindgen 0.2.130 comes from `~/.cargo/bin`).
- Measurements (fr1, wasm-opt 133, `--profile web`, `--slim --webgpu-only`, baseline cohort): table in the wasm-bindgen skill. Build: cargo 372 s under load 100+, wasm-bindgen 9 s, pre-opt module 25,935,137 B (code 8.3 MB, data 3.2 MB). Best brotli among flag sets: `-Oz --reorder-functions --reorder-locals --reorder-globals` 2,843,109 B vs 2,856,362 B today (-0.5 %), gzip -2.2 %; `-Oz --converge` 2,845,756 B at 316 s. Flag sets alone are not the lever.

## Remaining
1. fr1: `ssh fr1`, in a clone of fw: `cd configs/claude && bun scripts/toolchain.ts install --group wasm` (twiggy and cargo-component compile from source; wasi-sdk and emsdk download).
2. Not measured: SIMD cohort; `--traps-never-happen`, `--monomorphize`, `--inlining-optimizing`; `-Z build-std=std,panic_abort` with `-Cpanic=immediate-abort` and `-C target-feature=+simd128,+bulk-memory,...`; `-Zlocation-detail=none -Zfmt-debug=none`; fat LTO (`web-lto`); opt-level s vs z; twiggy `top/dominators/monos` on a `--debuginfo` build; `wasm-split` feasibility; wasm-bindgen `--remove-name-section --remove-producers-section --reference-types`.
3. First frame and code caching on the PC: harness already written in `D:\cache\dl-wasm\exp\` (`bench-serve.ts`, `bench.html`, `opt.ts`); copy a cohort dir to `D:\cache\dl-wasm\dist\<variant>\<cohort>\`, put `bench.html` as `index.html`, `bun bench-serve.ts D:/cache/dl-wasm/dist 4197`, open `/<variant>/?cohort=baseline`, read `window.__bench` (wasmFetchStart, wasmResponse, wasmCompiled, firstSubmit, firstFrame, adapter). Reload without `?cold=1` for the code cache check.
4. Only if a variant wins: edit `tooling/bun/web-build.ts` and `[profile.web]` in aphrody through a temp index on origin/main (no aphrody commit was made).

## Resume commands (fr1)
- Pre-opt wasm-bindgen output: `~/ci/wasm-opt/out/base/baseline/web_os_bg.wasm`; the simd cohort was never built: `~/ci/wasm-opt/build.sh base simd web ~/ci/target-wasm`.
- Variants: `~/ci/wasm-opt/exp/variants.txt`, runner `bun ~/ci/wasm-opt/exp/opt.ts <wasm> baseline <label> -- <flags>`, results `~/ci/wasm-opt/exp/opt-out/results.jsonl`.
- Workspace: `~/ci/wasm-opt/ws/aphrody` (clone of origin main at f48a99a) with `ws/fw` symlinked to `~/ci/wasm/fw`. Move CARGO_TARGET_DIR to `/srv/xbuild/<task>/` (root disk was 94-99 % full).

## Directories created
- fr1 `~/ci/wasm-opt` 255 MB (ws clone, out, exp, tc copy of toolchain files in `fw/`): safe to delete once the handoff is consumed.
- fr1 `~/ci/dl` 145 MB (release archives for listing): safe to delete.
- fr1 `~/ci/target-wasm` 4.4 GB: pre-existing CI cache, now holds extra artifacts of the `base` build (not deleted); not mine to remove.
- fr1 `~/.local/bin` wasm-* tools: keep (pinned). A stale `.toolchain-cargo-component-*` was removed.
- PC `D:\cache\dl-wasm` (release archives, section.toml, exp harness): archives safe to delete, `exp\` is the harness.
- PC `~/.local/opt/wasi-sdk-34.0` and `~/.local/bin` wasm tools: keep (pinned).
- No background jobs left running by this task.
