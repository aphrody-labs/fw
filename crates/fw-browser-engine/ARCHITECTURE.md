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
| `kitesurf` | Boa (Rust) in isolates | Stylo, Taffy, Parley, Blitz patched (193 patches) | Vello (CPU) | CDP, partial | fork aphrody-labs/kitesurf (pinned), Cloudflare's engine on Blitz; see [docs/kitesurf.md](docs/kitesurf.md) |
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

## Decision

Measurements (start-up, memory, scores, per engine, host and version) are private data of whoever ran them: `bun testing/acceptance.ts
bench` records them, `bun testing/acceptance.ts docs` writes `.local/docs/measurements.md`. The decision rests on what each engine
is, not on a number kept here:

- Blink is the engine of record for anything that renders: the only one with the full CDP, DevTools, Chrome-for-Testing parity
  (Playwright and Puppeteer work unchanged) and full web compatibility. Servo costs as much memory as Blink and is slower.
- The lite engine wins on memory by an order of magnitude for DOM, network and script work. It is Obscura on html5ever, cssparser,
  selectors/stylo and url from this repository, with V8 from our Chromium checkout instead of rusty_v8 prebuilts.
- agent-browser is the AI front end: snapshot, refs, click; it speaks CDP, so it drives either CDP engine.
- Servo's remaining value is its crates: stylo, html5ever, cssparser, url, webrender, mozjs, ipc-channel. Blitz uses Stylo too.
- Kitesurf is the Rust engine with the most work behind it: it turns Blitz into an engine that runs page JavaScript behind CDP, and carries the
  patches that close most of Blitz's gap with Blink. It is not Chromium and does not replace it: Blink stays the engine of record, and
  Kitesurf's value is the lite and edge path whose parity with Chromium 160 is measured on the same WPT corpus (docs/kitesurf.md).
- A verdict is about what the engine did, not what the server answered: `refused`, `auth` and `consent` are not failures, a status code
  is never trusted for a wall (the title and the final URL are read), an engine that hangs is a result with a timeout, and what an
  engine cannot do is recorded where the router reads it (`lessons`) and forgotten the day it can.

## Engines from the Chromium checkout

V8, Skia, Dawn and Blink are never taken as published crates or prebuilts (no rusty_v8 archive, no skia-safe binaries).
`build/engines/` drives GN on the Chromium checkout (`CHROMIUM_SRC`; the pinned revision is in `DEPS`):

- `//v8:v8_monolith` and `//skia:skia` static libraries, optimized official build (ThinLTO).
- Blink through `//headless:headless_shell` (native CDP, `--remote-debugging-pipe`); `headless_shell_lib` for in-process embedding.
- The Rust side links the libraries through the C ABI (`components/v8-sys`, `components/skia-sys`, generated with bindgen from the
  checkout headers) or cxx, the way Chromium does Rust interop.

## Layout

The tree follows Chromium. `components/<crate>` is first party (`//components`). Forks with a build of their own (the Bun
runtime, kitesurf, browse, mullvad) are pinned by commit in `tools/layout.toml` [forks] and built in their own checkout, never
submodules here; every other upstream is pinned in `tools/upstreams.toml` and fetched by Cargo, no tree on disk. Chromium's own
checkout is `third_party/chromium` on the build host, never committed. Then `build/`, `testing/` (`//testing`), `tools/`,
`styleguide/`, `docs/`, `DEPS` and `PRESUBMIT.ts`. One Cargo workspace for what cargo builds here, one `Cargo.lock`; a crate of ours
that needs one of Bun's depends on it by path and stays a non-member.

`tools/layout.toml` decides where everything goes (`bun tools/layout.ts` checks it), `tools/upstreams.toml` lists the upstreams,
`tools/upstreams.lock.json` records the pinned SHAs, `tools/layout.toml` [forks] pins the forks, `DEPS` pins Chromium,
`tools/import.py` imports and re-syncs (replayed linearly: `main` requires a linear history), `tools/flatten.py` keeps the
workspace manifest in step.
