# Chromium from Rust and Bun

`ay-chromium` embeds Chromium through CEF (ADR-3). One Rust crate (`crates/ay-chromium`), one executable
(`ay-chromium`), and one Bun package (`packages/chromium`, `@aphrody/chromium`) give two operations on a real page, rendered off-screen:

- **screenshot**: open a URL, wait for the main frame to load, write a PNG of the viewport;
- **evaluate**: open a URL, wait for the load, run a JavaScript expression in the page and return the value as JSON.

Proven on the test host (Linux, `<test-host>`): see "Proof" below. The inherited upstream CI failures and bugs of the four
forks behind this are in `research/chromium-rust-issues.toml`.

## Versions (read live, 2026-11-16)

| Part | Version | Source |
|---|---|---|
| CEF | 156.0.30 (`+g6028375+chromium-156.0.8078.96`) | `cef::sys::CEF_VERSION`, printed by `ay-chromium version` |
| cef (cef-rs) | 156.6.0+156.0.30, pinned `=156.6.0` | crates.io; fork `aphrody-labs/cef-rs` branch `dev` is the same code |
| cef-dll-sys | 156.6.0+156.0.30 | pulled by `cef` |
| download-cef | 3.0.1 | crates.io |
| napi (Bun addon) | 3.16.2, napi-derive 3.7.1, napi-build 2.6.0 | crates.io |
| Bun | 1.4.5 | `bun --version` on <test-host> |

## How it works

The crate depends on the published `cef` crate, which downloads the matching prebuilt CEF distribution during the
build (`cef-dll-sys`' `build.rs`, through `download-cef`; `CEF_PATH` caches it across builds). At run time:

1. `CefDir::locate` finds the runtime (`libcef.so`, `.pak` files, `locales/`): explicit `Config.cef_dir`, then
   `AY_CHROMIUM_CEF_DIR`, then `CEF_PATH` (and its `<version>/<target>` subdirectory), then the directories next to the
   helper executable, then the directory the crate was built against.
   `ay-chromium install [dir]` downloads and extracts the matching build with `download-cef` when there is none.
2. `Chromium::start` boots CEF once on a dedicated thread with `multi_threaded_message_loop`, windowless rendering
   and, on Linux, `--ozone-platform=headless` and software rendering, so no display server is needed. CEF re-launches
   a helper executable for its renderer, GPU and utility processes: `ay-chromium` itself (its `main` calls
   `subprocess_main` first), or, inside Bun, the `ay-chromium` binary next to the addon.
3. `open` posts a task to CEF's UI thread that creates one browser and returns when the main frame's load event
   fires (a watchdog turns a stuck load into a timeout error). On that page, `screenshot` sets the viewport
   (`was_resized`), invalidates the view and takes the next `OnPaint` BGRA frame of the requested size, encoded with
   the `png` crate; `evaluate` sends `Runtime.evaluate` with `returnByValue` and `awaitPromise` through
   `execute_dev_tools_method` and reads the reply in a `DevToolsMessageObserver`. Dropping the page closes the
   browser. State lives in `Rc`s on the UI thread; only `Send` data crosses to the caller.

## Rust API

```rust
use ay_chromium::{Chromium, Config, EvaluateOptions, ScreenshotOptions};

fn main() -> ay_chromium::Result<()> {
    // The executable CEF re-launches must call this first thing; it returns Some(code) in a child process.
    if let Some(code) = ay_chromium::subprocess_main() { std::process::exit(code); }

    let chromium = Chromium::start(Config::default())?;
    let title = chromium.evaluate("https://example.com", "document.title", &EvaluateOptions::default())?;
    let shot = chromium.screenshot("https://example.com", "/tmp/x.png", &ScreenshotOptions::default())?;
    println!("{title} {}x{}", shot.width, shot.height);
    Ok(())
}
```

| Item | Meaning |
|---|---|
| `Config { cef_dir, helper, cache_dir }` | Where CEF and the helper executable are, and the profile and log directory (a temporary one by default). |
| `ScreenshotOptions { width, height, settle, timeout }` | Defaults 1280, 720, 500 ms, 30 s. |
| `EvaluateOptions { settle, timeout }` | Defaults 0 ms, 30 s. |
| `Chromium::start(config)` / `Chromium::global(config)` | A new engine, or the process-wide one. CEF initializes once per process. |
| `trait Backend { fn open(&self, url) -> Result<Self::Page> }`, `trait Page { fn screenshot(&self, out, &opts); fn evaluate(&self, expression, &opts) }` | The engine-neutral shape for the future `ay_browser` crate; `Chromium` and `ChromiumPage` implement it. A page keeps its state between calls and is closed on drop. |
| `Chromium::screenshot(url, out, &opts) -> Screenshot { path, width, height }` | Opens, writes the PNG, closes. |
| `Chromium::evaluate(url, expression, &opts) -> serde_json::Value` | Opens, evaluates, closes. `undefined` is `null`. |
| `Chromium::open_with(url, timeout)` | `Backend::open` with a load timeout. |
| `Chromium::shutdown()` | Waits up to 5 s for open pages, then shuts CEF down; it cannot be started again in the process. Drop pages first. |
| `Error` | `CefNotFound`, `Start`, `Load` (failed navigation), `Timeout`, `Script` (a thrown exception), `Protocol`, `Png`, `Io`, `Closed`. |
| `install_cef(base)`, `cef_version()`, `CefDir::locate` | The CEF distribution. |

## CLI

```
ay-chromium screenshot <url> <out.png> [--width N] [--height N] [--wait MS] [--timeout S]
ay-chromium eval <url> <expression> [--wait MS] [--timeout S]
ay-chromium install [dir]
ay-chromium version
```

`screenshot` prints `{"path":...,"width":...,"height":...}`; `eval` prints the JSON value. Errors go to stderr with
exit code 1.

## Bun

```ts
import { screenshot, evaluate, shutdown } from "@aphrody/chromium";

await screenshot("https://example.com", "/tmp/x.png", { width: 1280, height: 720 });
const title = await evaluate<string>("https://example.com", "document.title"); // "Example Domain"
shutdown();
```

The package loads the Rust crate as a Node-API addon (`process.dlopen` of `native/ay_chromium.node`). Bun's FFI page
marks `bun:ffi` experimental with known bugs and names Node-API the most stable way to call native code, so the addon
is Node-API (napi-rs). Calls run on the libuv thread pool and return promises, so they never block the event loop.
`bun run build` (in `packages/chromium`) builds the helper (`--bins`) and the addon (`--lib --features node`, two runs: the Node-API symbols resolve only inside Bun) and copies both
into `native/`; the environment overrides are `AY_CHROMIUM_HOME`, `AY_CHROMIUM_ADDON`,
`AY_CHROMIUM_HELPER`, `AY_CHROMIUM_CEF_DIR` and `AY_CHROMIUM_CACHE_DIR`. The addon and the helper are not committed;
the test is `packages/chromium/test/chromium.test.ts`.

## What exists in the community, and what this reuses

Read live on 2026-11-16 before writing the glue:

| Project | State | Verdict |
|---|---|---|
| [`cef`](https://crates.io/crates/cef) / cef-rs ([tauri-apps/cef-rs](https://github.com/tauri-apps/cef-rs)) | 156.6.0+156.0.30 released 2026-11-16, 492 stars, active | **Reused**: the bindings, the `wrap_*` handler macros, the build-time CEF download and linking. Its `cefsimple` and `osr` examples gave the app, browser-process and render-handler structure. It ships no screenshot or evaluate API: the examples open a window or paint to a wgpu texture. |
| [`download-cef`](https://crates.io/crates/download-cef) 3.0.1 | 2026-11-15 | **Reused** for `ay-chromium install`, the same index and archive the crate build uses. |
| [`chromiumoxide`](https://github.com/mattsse/chromiumoxide) 0.9.1 | last release 2026-02-25, last push 2026-04-03, 15 open PRs, open bugs `save_screenshot` hangs (#321) and `screenshot()` steals focus (#308) | **Not used here.** An async CDP client that launches or connects to a separate Chrome binary (Chrome for Testing, headless shell) with tokio and the full CDP type crate. It gives screenshot and evaluate, but not the embedded CEF in our process that ADR-3 chooses for the native browser. It is the natural second `Backend` for `ay_browser`. I did not try attaching it to CEF's remote debugging port. |
| [`headless_chrome`](https://github.com/rust-headless-chrome/rust-headless-chrome) 1.0.22 | released 2026-06-11 | **Not used here**, same reason: it drives an external Chrome over CDP. |
| `png` 0.18.1, `serde_json`, `thiserror` | current | **Reused** for the PNG encoding, JSON and errors. |
| napi 3.16.2 / napi-derive 3.7.1 / napi-build 2.6.0 ([napi-rs](https://napi.rs)) | 2026-11-11 | **Reused** for the Bun addon. |

Community PRs and forks of cef-rs checked against what this crate calls, none applied because none touches a path used here:

- [#508](https://github.com/tauri-apps/cef-rs/pull/508) (empty `CefString` out-parameters are passed as a valid pointer): the crate passes no `string_byref`.
- [#516](https://github.com/tauri-apps/cef-rs/pull/516) (free `CefStringMultimap` and strings read from collections) and [#497](https://github.com/tauri-apps/cef-rs/pull/497) (keep owned strings when converting to raw CEF strings): the crate builds no string collections; its strings are passed by reference for the duration of each call.
- [#469](https://github.com/tauri-apps/cef-rs/pull/469) (better `wrap_*` developer experience), [#250](https://github.com/tauri-apps/cef-rs/pull/250) (input in the osr example), [#365](https://github.com/tauri-apps/cef-rs/pull/365) (Dawn GPU backend, WIP): not needed for software off-screen rendering.
- The chromiumoxide PRs [#332](https://github.com/mattsse/chromiumoxide/pull/332) / [#336](https://github.com/mattsse/chromiumoxide/pull/336) (command timeouts), [#338](https://github.com/mattsse/chromiumoxide/pull/338) (attach to targets that become pages later) and [#325](https://github.com/mattsse/chromiumoxide/pull/325) (`--remote-debugging-pipes`) are the ones a chromiumoxide backend would need; they are unmerged upstream.

If one of these starts to matter, the route is `[patch.crates-io] cef = { git = "https://github.com/aphrody-labs/cef-rs" }` with the PR branch.

## Build requirements

- Linux x86_64 (the target proven here), cmake and ninja (the `cef-dll-sys` build script), a Rust toolchain with
  edition 2024, about 1.3 GB of disk for the extracted CEF (333 MB download for the minimal distribution) plus the
  Rust target directory, network access to `cef-builds.spotifycdn.com` on the first build.
- Heavy builds run on the `build` host (<build-host>), tests on the `test` host (<test-host>) (`ay.toml` `[hosts]`), never on the Windows
  PC. Keep the CEF download, the target directory and the checkout on the build volume, `$WORK/<dir>` on <build-host> (`WORK` is `[paths.build].work` of `ay.toml`, `bun tools/hosts.ts work build`),
  which <test-host> mounts at `[paths.test].work`/`<dir>`; the roots of both hosts are too small for 1.3 GB of CEF plus a
  target directory. The Windows and macOS targets of `cef` are not exercised here: the OS-specific parts (the
  `libcef.so` check, the headless switches, `rpath`) are Linux code.
- Bun 1.4 for the package.

```sh
ssh <build-host>
cd $WORK/<dir>/ay                        # a checkout on the build volume
export CARGO_INCREMENTAL=0 CEF_PATH=$WORK/<dir>/cef CARGO_TARGET_DIR=$WORK/<dir>/target
cargo build --release -p ay-chromium
cargo test -p ay-chromium --release            # unit tests and the end-to-end test against a local page
bun --cwd packages/chromium run build && bun test packages/chromium
# on <test-host>, the same binaries through the NFS mount, `WORK_TEST` = `[paths.test].work` (libcef.so sits next to them, found through $ORIGIN):
$WORK_TEST/<dir>/target/release/ay-chromium screenshot https://example.com /tmp/x.png
```

## Proof (2026-11-16)

Built and tested on <build-host> in `$WORK/shared/ay-chromium`, run on <test-host> from the same path over NFS.

- <test-host>: `ay-chromium screenshot https://example.com /tmp/ay-chromium-example.png` printed
  `{"path":"/tmp/ay-chromium-example.png","width":1280,"height":720}`; the copy is
  `$AY_DATA_DIR/shots/ay-chromium-example.png` (the rendered example.com page, inspected).
- <test-host>: `ay-chromium eval https://example.com document.title` printed `"Example Domain"`.
- <build-host>: `cargo test -p ay-chromium --release`: 6 unit tests and
  `tests/e2e.rs::screenshot_and_evaluate_a_real_page` (real CEF against a local HTTP server: evaluate string, number,
  promise, object and exception, a load error, a PNG whose first pixel is the page's background colour, and one page
  reused for several evaluations and screenshots at three viewport sizes through the `Backend` and `Page` traits).
- <build-host> and <test-host>: `bun test` in `packages/chromium`: 4 pass, 0 fail, the same operations through the addon.
- <build-host>: `cargo clippy -D warnings` (stable) on the crate, plain and with `--lib --features node`, and `cargo fmt --check`.

## Not used yet: rusty_v8

`rusty_v8` (the `v8` crate) embeds a standalone V8 for code that runs outside a page. Both operations here run inside
the page's own V8 through the DevTools protocol, so adding it would only ship a second V8: the crate is at 152.2.0
while CEF 156 embeds the V8 of Chromium 156, and its `Update V8` workflow has failed every run since the `build`
submodule fetch breaks (`research/chromium-rust-issues.toml`). Add it when a feature needs V8 isolates without a page.

## CI

`bun run check` runs the repository-wide Bun, ABI, path, and feature-matrix gates. The `rust` job of `.github/workflows/check.yml` formats and lints the workspace, which includes this crate, so its build
script fetches CEF on the runner (333 MB download, 1.3 GB extracted, within the runner's disk). Its tests start
Chromium, which needs the system libraries CEF links against, so the job runs `cargo test --workspace --exclude
ay-chromium` and the crate's tests run on <test-host> with the commands above. That CI job has not run yet: it is unproven
until the first push.

## Limits

- CEF starts once per process; a second `Chromium::start` in the same process fails.
- Software rendering only (`--disable-gpu`): screenshots do not exercise WebGL or WebGPU.
- One viewport screenshot per call; no full-page capture, no cookies or input events yet.
- Chromium prints a harmless D-Bus `UPower` error on hosts without that service.
