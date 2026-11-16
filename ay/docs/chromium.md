# Chromium from Rust and Bun

`ay_chromium` embeds Chromium through CEF (ADR-3). One Rust crate (`crates/ay_chromium`), one executable
(`ay-chromium`), one Bun package (`packages/chromium`, `@aphrody/chromium`) and one skill
(`.claude/skills/chromium`) give two operations on a real page, rendered off-screen:

- **screenshot**: open a URL, wait for the main frame to load, write a PNG of the viewport;
- **evaluate**: open a URL, wait for the load, run a JavaScript expression in the page and return the value as JSON.

Proven on the test host (Linux, `fr2`): see "Proof" below. The inherited upstream CI failures and bugs of the four
forks behind this are in `research/chromium-rust-issues.toml`.

## Versions (read live, 2026-11-16)

| Part | Version | Source |
|---|---|---|
| CEF | 156.0.30 (`+g6028375+chromium-156.0.8078.96`) | `cef::sys::CEF_VERSION`, printed by `ay-chromium version` |
| cef (cef-rs) | 156.6.0+156.0.30, pinned `=156.6.0` | crates.io; fork `aphrody-labs/cef-rs` branch `dev` is the same code |
| cef-dll-sys | 156.6.0+156.0.30 | pulled by `cef` |
| download-cef | 3.0.1 | crates.io |
| napi (Bun addon) | 3.16.2, napi-derive 3.7.1, napi-build 2.6.0 | crates.io |
| Bun | 1.4.5 | `bun --version` on fr2 |

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
3. Each call posts a task to CEF's UI thread that creates one browser. A load handler waits for the main frame's
   load event, then (after `settle`) either invalidates the view and takes the next `OnPaint` BGRA frame
   (screenshot, encoded with the `png` crate), or sends `Runtime.evaluate` with `returnByValue` and `awaitPromise`
   through `execute_dev_tools_method` and reads the result in a `DevToolsMessageObserver` (evaluate). The browser is
   closed after the reply, and a watchdog turns a stuck page into a timeout error.

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
| `Chromium::screenshot(url, out, &opts) -> Screenshot { path, width, height }` | Writes the PNG. |
| `Chromium::evaluate(url, expression, &opts) -> serde_json::Value` | `undefined` is `null`. |
| `Chromium::shutdown()` | Closes the browsers and shuts CEF down; it cannot be started again in the process. |
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

## Build requirements

- Linux x86_64 (the target proven here), cmake and ninja (the `cef-dll-sys` build script), a Rust toolchain with
  edition 2024, about 1.3 GB of disk for the extracted CEF (333 MB download for the minimal distribution) plus the
  Rust target directory, network access to `cef-builds.spotifycdn.com` on the first build.
- Heavy builds run on the `build` host, tests on the `test` host (`ay.toml` `[hosts]`), never on the Windows PC.
  The Windows and macOS targets of `cef` are not exercised here: the OS-specific parts (the `libcef.so` check, the
  headless switches, `rpath`) are Linux code.
- Bun 1.4 for the package.

```sh
ssh fr2
df -h /                                        # check the free space first
export CARGO_INCREMENTAL=0 CEF_PATH=$HOME/.local/share/cef
cargo build --release -p ay_chromium
cargo test -p ay_chromium --release            # unit tests and the end-to-end test against a local page
(cd packages/chromium && bun run build && bun test)
```

## Proof (fr2, 2026-11-16)

- `ay-chromium screenshot https://example.com /tmp/x.png` wrote a 1280x720 PNG; the copy is
  `C:\data\shots\ay-chromium-example.png` (the rendered example.com page, inspected).
- `ay-chromium eval https://example.com document.title` printed `"Example Domain"`.
- `cargo test -p ay_chromium`: 6 unit tests and `tests/e2e.rs::screenshot_and_evaluate_a_real_page`
  (real CEF against a local HTTP server: evaluate string, number, promise, object and exception, a load error, and a PNG
  whose first pixel is the page's background colour).
- `bun test` in `packages/chromium`: the same operations through the addon.

## Not used yet: rusty_v8

`rusty_v8` (the `v8` crate) embeds a standalone V8 for code that runs outside a page. Both operations here run inside
the page's own V8 through the DevTools protocol, so adding it would only ship a second V8: the crate is at 152.2.0
while CEF 156 embeds the V8 of Chromium 156, and its `Update V8` workflow has failed every run since the `build`
submodule fetch breaks (`research/chromium-rust-issues.toml`). Add it when a feature needs V8 isolates without a page.

## CI

The `rust` job of `.github/workflows/check.yml` formats and lints the workspace, which includes this crate, so its build
script fetches CEF on the runner (333 MB download, 1.3 GB extracted, within the runner's disk). Its tests start
Chromium, which needs the system libraries CEF links against, so the job runs `cargo test --workspace --exclude
ay_chromium` and the crate's tests run on fr2 with the commands above. That CI job has not run yet: it is unproven
until the first push.

## Limits

- CEF starts once per process; a second `Chromium::start` in the same process fails.
- Software rendering only (`--disable-gpu`): screenshots do not exercise WebGL or WebGPU.
- One viewport screenshot per call; no full-page capture, no cookies or input events yet.
- Chromium prints a harmless D-Bus `UPower` error on hosts without that service.
