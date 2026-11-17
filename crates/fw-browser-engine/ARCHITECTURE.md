# Architecture

One runtime, many engines. Bun, our fork, is the embedder: it owns the process, the event loop, the JavaScript host and
`Bun.WebView`, and it can switch the engine underneath a page. The engines are the ones that matter in 2026:
WebKit with JavaScriptCore, Blink with V8, Skia and Dawn, and the Rust ones, Blitz, Servo and Obscura, with the experimental
Rust ecosystem (Boa, Nova, Ladybird) followed upstream.

```
 agent / Playwright / Puppeteer / DevTools frontend / Bun.WebView
                         | CDP (WebSocket or pipe)
                 +-------v--------+
                 | components/cdp |   one CDP endpoint, Chrome-for-Testing compatible (planned)
                 +-------+--------+
                         | components/browser-engine: catalog, pick, escalate
   +----------+----------+----------+----------+----------+---------------------------+
   | obscura  |  blitz   |  webkit  |  servo   |  blink   | boa, nova, ladybird       |
   | V8, lite | no JS of | JSC,     | SpiderM. | V8, Skia | watched (track), not run  |
   | no paint | its own  | WebCore  | WebRender| Dawn     |                           |
   +----------+----------+----------+----------+----------+---------------------------+
```

## The backends

| Backend | JavaScript | Style and layout | Paint, GPU | Protocol | Here |
|---|---|---|---|---|---|
| `webkit` | JavaScriptCore | WebCore | WebKit, system GPU layer | none (WKWebView) | Bun's native engine (`Bun.WebView`, macOS) |
| `blink` | V8 | Blink (LayoutNG) | Skia, Dawn (WebGPU) | CDP, full | Chromium checkout, `//headless:headless_shell` (`build/engines`) |
| `blitz` | none: the host runs the script | Stylo, Taffy, Parley | Vello on wgpu | none | `DioxusLabs/blitz`, tracked; its README: "no JavaScript bindings", beta |
| `servo` | SpiderMonkey (`mozjs`) | Stylo, Servo layout | WebRender, wgpu | Firefox RDP | crates in `third_party/servo` and its organisation's |
| `obscura` | V8 | html5ever, Stylo selectors | none (optional render) | CDP, subset | fork in `third_party/obscura` |
| `boa`, `nova` | Rust JavaScript engines | none | none | none | `track` only |
| `ladybird` | LibJS | LibWeb | Skia | none | `track` only |

`components/browser-engine` is this table as code: the catalog, `pick` (the lightest backend that satisfies what a page needs)
and `escalate` (the next heavier one when the first fell short). The rule is lite first: Obscura for script and DOM work,
Blitz when a static page needs pixels, WebKit when the host's JavaScriptCore is wanted, Servo for the Rust stack, Blink when a
page needs the full CDP, media or anything the others lack. Where an engine falls short is learned from the runs
(`bun testing/acceptance.ts lessons`, `docs/third-party-lessons.md`) and is the data of `escalate`.

Switching is `Bun.WebView({ backend })`: `webkit` and `chrome` exist in the runtime today; `blitz`, `servo` and `obscura` join
as backends of the same API, CDP where the engine speaks it.

## Decision (measured, 2026-11-16, fr1, one real page: en.wikipedia.org/wiki/Servo_(software), median of 5)

| Engine (prebuilt baseline)          | wall (s) | peak RSS, process tree (MiB) | CDP | Output |
|-------------------------------------|---------:|-----------------------------:|-----|--------|
| Chrome 160.0.8164.0 headless-shell  | 0.49 dump-dom / 0.59 screenshot | 578 / 584 | native, full | PNG 154 KB |
| Servo 0.8.0-rc1 servoshell          | 0.89 screenshot | 556 | none (Firefox RDP devtools) | PNG 332 KB |
| Obscura 0.2.7 no-render             | 0.66 fetch html | 69  | subset | HTML |
| Obscura 0.2.7 render                | 1.71 fetch html | 116 | subset | HTML, optional PNG |

Re-measured with hyperfine through `bun testing/acceptance.ts bench`: Chrome 483 ms and 518 MiB, Obscura 1.65 s and 115 MiB
(run count and spread in the database). Numbers are baselines of upstream releases, to be re-run on our builds.

- Blink is the engine of record for anything that renders: the only one with the full CDP, DevTools, Chrome-for-Testing parity
  (Playwright and Puppeteer work unchanged) and full web compatibility. Servo matches Chrome on memory and is slower.
- The lite engine wins on memory: 69 MiB against 578 MiB (8.4x). It is Obscura on html5ever, cssparser, selectors/stylo and url
  from this repository, with V8 from our Chromium checkout instead of rusty_v8 prebuilts.
- agent-browser is the AI front end: snapshot, refs, click; it speaks CDP, so it drives either CDP engine.
- Servo's remaining value is its crates: stylo, html5ever, cssparser, url, webrender, mozjs, ipc-channel. Blitz uses Stylo too.

## Engines from the Chromium checkout

V8, Skia, Dawn and Blink are never taken as published crates or prebuilts (no rusty_v8 archive, no skia-safe binaries).
`build/engines/` drives GN on the Chromium checkout (`CHROMIUM_SRC`, default `/srv/chromium/src`; the pinned revision is in `DEPS`):

- `//v8:v8_monolith` and `//skia:skia` static libraries, optimized official build (ThinLTO).
- Blink through `//headless:headless_shell` (native CDP, `--remote-debugging-pipe`); `headless_shell_lib` for in-process embedding.
- The Rust side links the libraries through the C ABI (`components/v8-sys`, `components/skia-sys`, generated with bindgen from the
  checkout headers) or cxx, the way Chromium does Rust interop.

## Layout

The tree follows Chromium. `third_party/bun` is the runtime: a git submodule of our fork (relative URL in `.gitmodules`), a
workspace and a build of its own (`bun bd`), placed where Chromium places a dependency. `components/<crate>` is first party
(`//components`). `third_party/<upstream>/<crate>` is everything imported, with a generated `README.chromium`. Chromium's own
checkout is `third_party/chromium` on the build host, never committed. Then `build/`, `testing/` (`//testing`), `tools/`,
`styleguide/`, `docs/`, `DEPS` and `PRESUBMIT.ts`. One Cargo workspace for what cargo builds here, one `Cargo.lock`; a crate of ours
that needs one of Bun's depends on it by path and stays a non-member.

`tools/layout.toml` decides where everything goes (`bun tools/layout.ts` checks it), `tools/upstreams.toml` lists the upstreams,
`tools/upstreams.lock.json` pins what is imported, `.gitmodules` and the gitlink pin Bun, `DEPS` pins Chromium,
`tools/import.py` imports and re-syncs (replayed linearly: `main` requires a linear history), `tools/flatten.py` keeps the
workspace manifest in step.
