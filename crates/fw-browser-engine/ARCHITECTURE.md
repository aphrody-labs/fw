# Architecture

One runtime, many engines. Bun, our fork, is the embedder: it owns the process, the event loop, the JavaScript host and
`Bun.WebView`, and it can switch the engine underneath a page. The engines: WebKit with JavaScriptCore, Blink with V8, Skia and Dawn, and the Rust ones, Blitz, Servo, Obscura and Kitesurf;
Boa, Nova and Ladybird are followed upstream.

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
| `kitesurf` | Boa (Rust) in isolates | Stylo, Taffy, Parley, Blitz patched (`scripts/patches/` of the fork) | Vello (CPU) | CDP, partial | fork aphrody-labs/kitesurf (pinned), Cloudflare's engine on Blitz; see [docs/kitesurf.md](docs/kitesurf.md) |
| `servo` | SpiderMonkey (`mozjs`) | Stylo, Servo layout | WebRender, wgpu | Firefox RDP | Servo and its organisation's crates, pinned in `tools/upstreams.toml` |
| `obscura` | V8 | html5ever, Stylo selectors | none (optional render) | CDP, subset | fork aphrody-labs/obscura, pinned in `tools/upstreams.toml` |
| `boa`, `nova` | Rust JavaScript engines | none | none | none | `track` only |
| `ladybird` | LibJS | LibWeb | Skia | none | `track` only |

`components/browser-engine` is this table as code: the catalog, `pick` (the lightest backend that satisfies what a page needs)
and `escalate` (the next heavier one when the first fell short). The rule is lite first: Obscura for script and DOM work,
Blitz when a static page needs pixels, WebKit when the host's JavaScriptCore is wanted, Servo for the Rust stack, Blink when a
page needs the full CDP, media or anything the others lack. Where an engine falls short is learned from the runs
(`bun testing/acceptance.ts lessons`, `docs/third-party-lessons.md`) and is the data of `escalate`.

Switching is `Bun.WebView({ backend })`: `webkit` and `chrome` exist in the runtime; `blitz`, `servo` and `obscura` are planned
as backends of the same API, CDP where the engine speaks it.

## Decision

Measurements (start-up, memory, scores, per engine, host and version) are private data of whoever ran them: `bun testing/acceptance.ts
bench` records them, `bun testing/acceptance.ts docs` writes `.local/docs/measurements.md`. The decision rests on what each engine
is, not on a number kept here:

- Blink is the engine of record for anything that renders: the only one with the full CDP, DevTools, Chrome-for-Testing parity
  (Playwright and Puppeteer work unchanged) and full web compatibility.
- The lite engine is for DOM, network and script work, where memory matters. It is Obscura on the pinned html5ever, cssparser,
  selectors/stylo and url crates, with V8 from our Chromium checkout instead of rusty_v8 prebuilts.
- agent-browser is the AI front end: snapshot, refs, click; it speaks CDP, so it drives either CDP engine.
- Servo's remaining value is its crates: stylo, html5ever, cssparser, url, webrender, mozjs, ipc-channel. Blitz uses Stylo too.
- Kitesurf turns Blitz into an engine that runs page JavaScript behind CDP and carries patches that close part of Blitz's gap with Blink.
  It is not Chromium and does not replace it: Blink stays the engine of record, and Kitesurf is the lite and edge path
  ([docs/kitesurf.md](docs/kitesurf.md)).
- A verdict is about what the engine did, not what the server answered: `refused`, `auth` and `consent` are not failures, a status code
  is never trusted for a wall (the title and the final URL are read), an engine that hangs is a result with a timeout, and what an
  engine cannot do is recorded where the router reads it (`lessons`) and forgotten the day it can.

## Engines from the Chromium checkout

V8, Skia, Dawn and Blink are never taken as published crates or prebuilts (no rusty_v8 archive, no skia-safe binaries).
`build/engines/` drives GN on the Chromium checkout (`CHROMIUM_SRC`; the pinned revision is in `DEPS`):

- `//v8:v8_monolith` and `//skia:skia` static libraries, optimized official build (ThinLTO).
- Blink through `//headless:headless_shell` (native CDP, `--remote-debugging-pipe`).
- The Rust side will link the libraries through the C ABI (`components/v8-sys`, `components/skia-sys`, generated with bindgen from the
  checkout headers; not written yet) or cxx, the way Chromium does Rust interop.

## Layout

The tree follows Chromium; [tools/layout.toml](tools/layout.toml) decides where everything goes (`bun tools/layout.ts` checks it) and
[AGENTS.md](AGENTS.md) lists the directories. Forks with a build of their own are pinned in `[forks]`, every other upstream in
`tools/upstreams.toml` and `tools/upstreams.lock.json`, Chromium in `DEPS`.
