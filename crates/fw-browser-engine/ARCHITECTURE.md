# Architecture

One browser, two engines behind one CDP front, on Bun's crate layout.

```
 agent / Playwright / Puppeteer / DevTools frontend
                     | CDP (WebSocket or pipe)
              +------v-------+
              |   src/cdp    |  one CDP endpoint, Chrome-for-Testing compatible
              +--+--------+--+
   lite engine   |        |   full engine
 (obscura-*, Servo crates)  |   (Blink, Chromium headless)
 DOM+net+JS, no paint       |   layout+paint+DevTools+media
 V8 from our Chromium build |   V8 + Skia from the same build
```

## Decision (measured, 2026-11-16, fr1, one real page: en.wikipedia.org/wiki/Servo_(software), median of 5)

| Engine (prebuilt baseline)          | wall (s) | peak RSS, process tree (MiB) | CDP | Output |
|-------------------------------------|---------:|-----------------------------:|-----|--------|
| Chrome 160.0.8164.0 headless-shell  | 0.49 dump-dom / 0.59 screenshot | 578 / 584 | native, full | PNG 154 KB |
| Servo 0.8.0-rc1 servoshell          | 0.89 screenshot | 556 | none (Firefox RDP devtools) | PNG 332 KB |
| Obscura 0.2.7 no-render             | 0.66 fetch html | 69  | subset | HTML |
| Obscura 0.2.7 render                | 1.71 fetch html | 116 | subset | HTML, optional PNG |

(`tools/bench/startup.py`; numbers are baselines of upstream releases, to be re-run on our builds.)

- Blink is the engine of record for anything that renders: it is the only one with the full CDP, DevTools,
  Chrome-for-Testing parity (Playwright and Puppeteer work unchanged) and full web compatibility. Servo
  matches Chrome on memory (556 vs 584 MiB) and is slower, so it does not win as a second renderer.
- The lite engine wins on memory: 69 MiB against 578 MiB (8.4x) for DOM, network and script work. It is
  Obscura on html5ever, cssparser, selectors/stylo, url from this monorepo, with V8 from our Chromium
  checkout instead of rusty_v8 prebuilts.
- agent-browser is the AI front end: snapshot, refs, click; it speaks CDP, so it drives either engine.
- Servo's remaining value is its crates: stylo, html5ever, cssparser, url, webrender, mozjs, ipc-channel.
  They are used where they win (lite engine parse/style/URL), not as a third engine.
- The router picks lite first and escalates to full when a page needs paint, media or a feature the lite
  engine lacks (the same CDP session is replayed on Blink).

## Engines from the Chromium checkout

V8, Skia and Blink are never taken as published crates or prebuilts (no rusty_v8 archive, no skia-safe
binaries). `build/engines/` drives GN on the Chromium checkout (`CHROMIUM_SRC`, default
`/srv/chromium/src`; milestone 157 to 160 is that one variable):

- `//v8:v8_monolith` and `//skia:skia` static libraries, optimized official build (ThinLTO, which is
  what Chromium ships; Chromium has no full-LTO mode for native targets).
- Blink through `//headless:headless_shell` (native CDP, `--remote-debugging-pipe`); `headless_shell_lib`
  when in-process embedding is wanted.
- Rust side links the libraries through the C ABI (`src/v8-sys`, `src/skia-sys`, generated with bindgen from
  the checkout headers) or cxx (vendored in `third_party/rust/chromium_crates_io`), the way Chromium
  does Rust interop. Cross-language LTO needs rustc and clang on the same LLVM (see `docs/`).

## Acceptance corpus

The Dioxus monorepo's web examples must render and stay interactive here, compared with Chrome for Testing.
They need WASM, WebGPU and CDP, so they run on the full engine (the lite engine does no paint).
`tools/acceptance/cdp-probe.ts` is the probe (raw CDP, no Playwright): on Chrome 160.0.8164.0 headless-shell it
reports WASM true and a WebGPU adapter (SwiftShader) for dioxuslabs.com (title, 208 elements) and for the
WebGPU hello-triangle sample (1 canvas, PNG rendered). The same flags
(`--enable-unsafe-webgpu --use-angle=swiftshader --use-webgpu-adapter=swiftshader`) apply to our
`headless_shell` build.

## Status (what exists, what does not)

- Done: history-preserving import of 30 upstreams, one workspace (311 members, `cargo metadata` resolves,
  one `Cargo.lock`), baseline measurements, CDP probe, GN overlay (`build/engines`).
- V8 + Skia + ICU archive: building on the build host from the Chromium 157 checkout (`browser:engines`).
- Not done: `src/v8-sys` and `src/skia-sys`; the Obscura script layer still depends on deno_core and
  rusty_v8 and is therefore outside the workspace (it cannot resolve next to mozjs_sys); the lite engine
  does not run yet; Blink (`browser:blink`) is not built; no PNG or CDP proof from our own binaries yet.

## Layout

`src/<crate>` flat, Bun style, one Cargo workspace, one `Cargo.lock`. Imported crates keep their package
names; `upstream/<name>/` holds what is not a crate (CI, docs, resources) with history preserved.
`tools/upstreams.toml` lists the upstreams, `tools/import.py` imports and re-syncs them, `tools/flatten.py`
rewrites the manifests.
