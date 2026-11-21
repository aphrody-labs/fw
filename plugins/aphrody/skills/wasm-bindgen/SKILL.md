---
name: wasm-bindgen
description: "Authoritative wasm-bindgen, js-sys, web-sys, wasm-bindgen-futures, walrus, weedle, ts-gen. Use when editing crates/gpui_web, crates/aphrody_desktop web builds, tooling/bun/web-build.ts"
---
> and Browser Wasm FFI skill for aphrody. , wasm32-unknown-unknown exports, Closure lifetimes, Uint8Array views, or optimizing Wasm binaries with walrus + wasm-opt.


# wasm-bindgen Ecosystem & Browser FFI (`aphrody`)

Aphrody pins `wasm-bindgen` at **`0.2.130`** (`Cargo.lock` == `wasm-bindgen` CLI `0.2.130`, `wasm-bindgen-futures`, `js-sys`, `web-sys` from the same lock) and optimizes release browser binaries with **`v133`** via [`tooling/bun/web-build.ts`](../../../../../tooling/bun/web-build.ts).

## Progressive Disclosure References

Resolve all paths relative to `.agents/plugins/aphrody/skills/wasm-bindgen/`:

| Topic | Reference File | When to Load |
| :--- | :--- | :--- |
| **8-Repo Upstream Matrix** | [references/repos-matrix.md](references/repos-matrix.md) | Checking live commits, MSRV, sub-crates, or `crates.io` drift across all 8 `github.com/wasm-bindgen` repositories |
| **JS/DOM/Futures FFI Patterns** | [references/js-web-sys-patterns.md](references/js-web-sys-patterns.md) | Writing `Closure` listeners, `Uint8Array` transfers, `JsFuture`/`spawn_local` loops, `web-sys` feature lists, or `Reflect` feature probes |
| **walrus, weedle, ts-gen & ABI R&D** | [references/walrus-weedle-tsgen.md](references/walrus-weedle-tsgen.md) | Inspecting Wasm IR passes (`walrus` -> `wasm-opt`), WebIDL parsing (`weedle`), `.d.ts` codegen (`ts-gen`), `js-bindgen`, or `Externref` RFC |
| **Aphrody Build & Architecture Rules** | [references/aphrody-best-practices.md](references/aphrody-best-practices.md) | Configuring `[profile.web]`, Windows `RUSTC_WRAPPER=""` workaround, 3-cohort builds, or contrasting `gpui_web` vs `nsh-platform-web` |
| **Canonical Architecture Doc** | [docs/architecture/wasm-bindgen-ecosystem.md](../../../../../docs/architecture/wasm-bindgen-ecosystem.md) | Full architectural synthesis grounded in `Cargo.lock`, `gpui_web`, and live GitIngest digests |
| **Standardized `llms.txt`** | [docs/architecture/wasm-bindgen-llms.txt](../../../../../docs/architecture/wasm-bindgen-llms.txt) | Machine-readable `llmstxt.org` index of all repos, guides, and skill references |

## Non-Negotiable Engineering Rules

1. **Exact CLI <-> `Cargo.lock` Lockstep**:
   `wasm-bindgen` CLI version MUST equal `Cargo.lock` (`0.2.130`). Never mix CLI and crate versions — `web-build.ts` aborts on mismatch because `__wbindgen_placeholder__` intrinsics and JS glue evolve across patch releases.
2. **Never Leak `Closure`s Unless Process-Lifetime**:
   - Repeating DOM listeners (`window`, `document`, `ResizeObserver`, `MediaQueryList`): wrap in `EventListenerHandle` (`Drop` calls `remove_event_listener_with_callback`, see [`crates/gpui_web/src/events.rs`](../../../../../crates/gpui_web/src/events.rs)) or store the `Closure` on the owning struct (`WebWindow`).
   - One-shot callbacks (`setTimeout`, `queueMicrotask`, `requestIdleCallback`): prefer `Closure::once_into_js`; never call `.forget()` in per-event or per-frame loops.
3. **Never Hold `Uint8Array::view` Across Any Rust Allocation**:
   `unsafe { js_sys::Uint8Array::view(slice) }` points directly into Wasm linear memory. Any heap growth (`Box`, `Vec::push`, `String`, JS->Rust callback) can execute `memory.grow`, detaching `memory.buffer` and invalidating the JS view. Consume synchronously inside a single JS boundary call or copy via `Uint8Array::from(bytes)` / `.to_vec()`.
4. **Prevent `RefCell` Reentrancy Panics Under `panic = "abort"`**:
   Aphrody `[profile.web]` compiles with `panic = "abort"` — a `RefCell::borrow_mut` panic during a synchronous DOM callback permanently wedges the Wasm instance. Always extract callbacks with `.take()` -> invoke -> restore ([`WebWindowInner::with_callback`](../../../../../crates/gpui_web/src/window.rs)).
5. **Minimize `web-sys` Features & Override `RUSTC_WRAPPER=""` on Windows**:
   Enable only exact WebIDL interfaces needed (`52` features in `gpui_web`, `1` in `gpui_wgpu`). On Windows, unset `RUSTC_WRAPPER` during `wasm32` builds (`web-build.ts`) so `sccache` does not overflow `CreateProcess` command-line limits on `web-sys`.
6. **Use `cargo build + wasm-bindgen + wasm-opt v133`, Never Legacy `wasm-pack`**:
   Run `bun tooling/bun/web-build.ts` for deterministic `baseline`, `simd-baseline`, and opt-in `multithreaded` (`--coi`) cohorts without npm/webpack/COOP-required regressions.

## Fast Verification Commands

```powershell
# Check exact wasm-bindgen lockstep + compile gpui_web / gpui_wgpu / aphrody_desktop
bun tooling/bun/web-build.ts check

# Build optimized release cohorts (baseline + simd-baseline) with wasm-opt v133
bun tooling/bun/web-build.ts build --package aphrody_desktop --release --opt

# Refresh live 8-repo GitIngest digests, llms.txt, skills, and architecture telemetry
bun tooling/bun/wasm-bindgen-ingest.ts all

# Run unit + link integrity tests for the wasm-bindgen ingestion pipeline
bun test tooling/bun/wasm-bindgen-ingest.test.ts
```

## Pinned WebAssembly toolchain

`configs/claude/toolchain.toml` (group `wasm`, installed and verified by `bun configs/claude/scripts/toolchain.ts check|install --group wasm`, sha256 per asset, fails on drift) pins: wasm-bindgen 0.2.130 (`lock` = `aphrody/Cargo.lock`), Binaryen 133 (wasm-opt, wasm-merge, wasm-metadce, wasm-split, wasm-dis, wasm-as; `lock` = `BINARYEN_VERSION` in `aphrody-ci.yml`), wasm-tools 1.263.0, WABT 1.0.42, wit-bindgen 0.63.0, wasmtime 51.0.0, wasmer 7.5.0, twiggy 0.8.0 (built from the crates.io source), wasi-sdk 34.0, emsdk 7.0.0 (fleet; `7.0.1-git` is the development head, 7.0.0 the latest tag), cargo-component 0.21.1 (fleet). Not pinned: wasm-pack (no script calls it), jco (unused), wasm-snip (last release 2019-09-12).

## wasm-opt flag sets measured (fr1, wasm-opt 133, `--profile web`, `--slim --webgpu-only`, baseline cohort, wasm-bindgen output 25,935,137 B before optimization)

| flags (plus `--strip-producers --low-memory-unused --strip-debug`, the `--enable-*` set) | raw B | brotli B | gzip B | s |
| :--- | ---: | ---: | ---: | ---: |
| `-Oz --vacuum --dce` (web-build.ts today) | 10,101,565 | 2,856,362 | 3,882,655 | 175 |
| `-Oz --converge` | 10,081,472 | 2,845,756 | 3,870,345 | 316 |
| `-Oz --reorder-functions --reorder-locals --reorder-globals` | 10,082,380 | 2,843,109 | 3,799,110 | 92 |
| `-Oz --gufa-optimizing` | 10,103,255 | 2,856,662 | 3,883,162 | 140 |
| `-Oz --ignore-implicit-traps` | 10,088,576 | 2,854,128 | 3,875,604 | 127 |
| `-Os` | 10,311,386 | 2,854,709 | 3,882,678 | 174 |
| `-O3` | 10,360,493 | 2,857,669 | 3,894,807 | 119 |
| `-O4` | 10,368,451 | 2,871,596 | 3,900,959 | 126 |

Flag sets move brotli by at most 0.5 %; `--reorder-functions` is the only cheap gain (gzip -2.2 %). Larger wins have to come from the code itself (twiggy), not from Binaryen. `--traps-never-happen`, `--monomorphize`, `--inlining-optimizing`, their combinations, the SIMD cohort and the `build-std` and `panic_immediate_abort` builds were not measured.
