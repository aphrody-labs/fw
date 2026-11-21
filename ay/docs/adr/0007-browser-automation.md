# ADR-7: One browser-automation ecosystem, `ay browser`

Status: proposed, 2026-11-16. Evidence is in `docs/research/browser-automation.toml` and `.md`; the engine for the browser product is ADR-3 (`0003-browser.md`), whose CfT and headless-shell role for tests and agents this ADR makes concrete.

## Context
Eleven projects were read at the aphrody-labs forks (obscura, lightpanda, agent-browser, playwright, playwright-mcp, puppeteer, chrome-devtools-mcp, chromiumoxide, headless_chrome, Chrome for Testing, cef-rs). Facts that decide the design:

- CDP is the common protocol. agent-browser already drives Chrome for Testing, lightpanda and obscura through one CDP path (`--engine`), and aphrody-labs/browser already decided "one CDP front, two engines, router picks lite first" with measurements (69 MiB lite vs 578 MiB headless-shell on one page).
- The same ~25 verbs exist five times with five names. playwright-mcp's `browser_*` names are the de-facto convention (obscura copied them).
- Only Playwright has Firefox and WebKit, a test runner, codegen and a trace viewer. Only DevTools MCP has performance, Lighthouse, heap, extension, PWA and WebAuthn tools. Only obscura has stealth.
- No Rust library combines refs, tools and several backends: agent-browser is a binary without `[lib]`; chromiumoxide and headless_chrome are bare CDP clients; `ay-chromium` covers CEF only.
- lightpanda is AGPL-3.0-only; everything else read is Apache-2.0 or MIT.

## Decision (proposed)
One ecosystem, **ay browser**, assembled from upstreams, in six parts.

### 1. One Rust crate, `ay_browser` (`crates/ay_browser`)
- **Core**: a transport-agnostic CDP session (`Transport`: WebSocket, pipe, in-process) using the typed commands and events of `chromiumoxide_cdp` and `chromiumoxide_types` from crates.io; no hand-written protocol types.
- **`Backend` trait**: `launch(config) -> Session` and `capabilities() -> Capabilities`. `Capabilities` is a set (rendering, pdf, extensions, profiles, storage-state, devtools-performance, webmcp, stealth, bidi). Operations a backend lacks return `Unsupported(capability)`, never a silent no-op; this is the model agent-browser's lightpanda page approximates with flag errors.
- **Backends** (cargo features, one module each):
  - `chromium`: spawn Chrome for Testing `chrome` or `chrome-headless-shell` pinned by version from the CfT endpoints, over `--remote-debugging-pipe` or WebSocket. Default backend.
  - `lightpanda`: spawn `lightpanda serve` and speak CDP (or BiDi later); always out of process, which keeps AGPL code out of our binaries.
  - `obscura`: in-process through the `obscura` crate (git dependency, pinned `rev`, features `render` and `stealth`) if the probe shows it works from our tree; otherwise spawn `obscura serve`. The in-process route is conditional because the crate builds V8 from source and is not on crates.io.
  - `cef`: `ay-chromium` (below).
  - `remote`: attach to any CDP endpoint (`--cdp`, cloud providers).
- **Refs and snapshot**: one snapshot model (accessibility tree plus stable `@eN` refs, invalidated on navigation, covered-element click failure) implemented once in `ay_browser` over CDP `Accessibility` and `DOM`; obscura's in-process path maps to the same type.
- **Router**: lite first, escalate to Blink when a page needs paint, media or a missing capability, replaying the session (the `aphrody-labs/browser` design).
- Rust builds run on the build host (`ay.local.toml` [hosts]).

### 2. `ay-chromium` becomes the Chromium (CEF) backend
`crates/ay_chromium` (CEF 156.6.0 through cef-rs; screenshot through off-screen rendering and `Runtime.evaluate` through `execute_dev_tools_method` and a DevTools message observer, engine.rs lines 310 to 361) stays its own crate because CEF needs its helper binary and process model. It is the `cef` backend of `ay_browser` once it gains (a) a persistent browser per session instead of a fresh browser per call and (b) a raw CDP channel: send any method, receive results and events from the existing observer. Its public `screenshot` and `evaluate` then become thin calls on a session. The Node-API addon (`node` feature) is reused by the Bun package. The ADR-3 measurement list (CDP attach, extensions, WebMCP on CEF 156 and 158) gates how much of the DevTools-class toolset the `cef` backend advertises.

### 3. One CLI, `ay-browser` (also `ay browser`)
Names follow `ay.toml`. The command set is the deduplicated union of agent-browser (20 groups), the Playwright CLI (about 102 command names) and Puppeteer's `browsers` commands:
- Verbs and flags are taken from the Playwright CLI vocabulary where both exist (`goto`, `click`, `fill`, `route`, `cookie-*`, `localstorage-*`, `tracing-*`, `video-*`, `state-save`), with agent-browser's names accepted as aliases (`open`, `snapshot -i`, `find`, `diff`, `flow`, `batch`, `session`, `auth`, `skills`).
- Added from Puppeteer: `browsers install|launch|clear|list` (CfT and headless-shell by version).
- Added from lightpanda and obscura: `fetch` (`--dump html|markdown|text|links|assets|structured_data`), `scrape` (parallel), `serve` (CDP endpoint for Playwright, Puppeteer and DevTools), `mcp`.
- Not in v1: the Playwright test runner, VS Code extension and trace viewer (Playwright stays an external npm tool, see 5).
- Phase A: `ay-browser` ships as the agent-browser submodule binary plus an alias layer, because it has no `[lib]` and already implements most of the surface. Phase B: commands move onto `ay_browser` one group at a time, starting with those that are engine-agnostic; agent-browser's code is retired as each group reaches parity. Whether upstream would accept a `[lib]` split is unknown.

### 4. One MCP server, a superset of playwright-mcp and Chrome DevTools MCP
- Built into `ay-browser mcp` (stdio and HTTP, one session per `Mcp-Session-Id` as lightpanda does).
- Tool names: the 72 `browser_*` names of playwright-mcp are canonical; DevTools MCP tools that overlap (click, fill, fill_form, hover, press_key, navigate_page, new_page, wait_for, handle_dialog, resize_page, take_screenshot, take_snapshot, evaluate_script, list_console_messages, list_network_requests) are accepted as aliases; the remaining DevTools tools (performance, lighthouse_audit, 14 heap-snapshot tools, extensions, PWA, WebAuthn, logpoints, 3p tools, get_css_styles, screencast) keep their own names; obscura's 14 are a strict subset; extras from agent-browser and lightpanda (`extract`, `structured_data`, `detect_forms`, `diff_*`, `flow_*`, `auth_*`, `run`/`save` script replay, `session_*`) are added in capability groups.
- Capability groups are opt-in like playwright-mcp `--caps` (core, tabs, network, storage, devtools, vision, pdf, testing, debug, extensions, webmcp), and `tools/list` is filtered by the active backend's `Capabilities`, so a lightpanda session never advertises `browser_pdf` as raster.
- One registry file (tool, aliases, group, capability, schema) generates the MCP server, the CLI help and the docs reference. The registry is checked against the upstream catalogs read live (playwright-mcp README, DevTools `docs/tool-reference.md`, agent-browser `mcp-tools.json`); a missing upstream tool fails the check.
- Phase A may proxy DevTools-only tools to `chrome-devtools-mcp` run as an external npm dependency on Chromium backends, then reimplement over `ay_browser` only where the proxy is a limit.
- Telemetry: spawned or wrapped lightpanda and DevTools MCP processes are started with their opt-outs set (`LIGHTPANDA_DISABLE_TELEMETRY`, `--no-usage-statistics`, `--no-performance-crux`).

### 5. One npm package, `@aphrody/browser` (`packages/browser`, Bun)
- Compatibility by protocol, not reimplementation: `ay-browser serve` exposes a CDP endpoint, so `puppeteer-core` and `playwright-core` (`connectOverCDP`) run unchanged against every backend that serves CDP. `@aphrody/browser` exports `launch()` and `connect()` that start or attach a backend and return a Puppeteer `Browser` or Playwright `Browser` when the user has installed that package (optional peer dependency), and a small native `Browser`/`Page`/`Locator` API (goto, snapshot, click, fill, screenshot, evaluate, route, cookies) over the daemon otherwise. Everything beyond that subset (test runner, codegen, trace viewer, Firefox, WebKit) is Playwright itself, used as an external npm dependency.
- In-process path: the Node-API addon from `ay-chromium` (feature `node`) and `ay_browser`, loaded by Bun.
- Verified claim needed before release: that `puppeteer-core` 25.x and `playwright-core` 1.65 run on Bun against each backend (not run yet).

### 6. One skill and one docs reference
- Skill `ay-browser` in the agent-browser style: a thin stub whose instructions come from `ay-browser skills get core`, so the text always matches the installed binary and registry; specialised sub-skills (debug/performance from DevTools, auth, scraping, WebMCP) are generated from the registry, not hand-copied.
- One docs reference generated from the same registry (commands, tools, aliases, capabilities, per-backend support table). Upstream skills (agent-browser's 10, Playwright's 3, DevTools MCP's 8, obscura's 1, lightpanda's) are read as inputs, not shipped copies.

## Third-party policy
Upstreams are git submodules in `third_party/` (relative URLs, one checkout each, aphrody-labs forks synced with upstream) or crates and npm packages from registries with pinned versions; never copies (`AGENTS.md`). Concretely:
- Submodules: agent-browser, obscura, lightpanda, playwright, playwright-mcp, puppeteer, chrome-devtools-mcp (which itself submodules devtools-frontend), chromiumoxide, chrome-for-testing, cef-rs.
- Crates: `chromiumoxide_cdp`, `chromiumoxide_types`, `cef`, `download-cef`; `obscura` as a git dependency with a full `rev`.
- Not extracted from upstreams: obscura's `vendor/` (taffy, cosmic-text, wreq-util), agent-browser's bundled `axe.min.js` (MPL-2.0), lightpanda's `src/rust` and fonts, Puppeteer's `third_party` (mitt, parsel-js, rxjs, urlpattern-polyfill). When `ay_browser` needs the same functionality it depends on the original crate or package (taffy and cosmic-text on crates.io, `axe-core` on npm).
- Binaries (CfT, headless-shell, CEF, lightpanda, obscura releases) are downloaded and pinned by version and hash, not stored in git.
- lightpanda stays a spawned process. This is an engineering boundary (Zig binary, CDP) that also keeps AGPL-3.0-only code out of linked artifacts; licence terms of the downloaded Chrome, Firefox, WebKit and ffmpeg builds are unknown and are not decided here.

## Build order (most reuse first)
1. **Probe (reuse: everything, new code none).** On the build and test hosts, run agent-browser against CfT headless-shell 157, lightpanda 1.0.4, obscura 0.2.7 and a CEF 156 debugging port; record per-command pass or fail into the capability matrix; run puppeteer-core and playwright-core against `serve` of each; one benchmark table on <test-host> across engines (replaces vendor figures).
2. **Submodules and pins** for the list above; `tools/` check that the pin matches a live upstream release.
3. **`ay-browser` Phase A**: agent-browser binary plus alias layer, `browsers` commands, `serve` passthrough (reuses almost all of the surface; needs only the alias table).
4. **`ay_browser` core**: Transport, Session, `Capabilities`, `chromium` and `remote` backends on `chromiumoxide_cdp` types, snapshot with refs; then `lightpanda` and `obscura` (spawn) backends.
5. **`ay-chromium` upgrade** to a persistent session with a raw CDP channel, then the `cef` backend.
6. **Tool registry and MCP server**, DevTools-only tools proxied to `chrome-devtools-mcp` first.
7. **`@aphrody/browser`**: `launch`/`connect` returning Puppeteer or Playwright objects, then the native subset and the napi addon.
8. **Skill and docs reference** generated from the registry.
9. **`ay-browser` Phase B** migration to `ay_browser`, router (lite first, Blink escalation), obscura in-process, stealth over `remote` where feasible.

## Open questions (not decided here)
1. `aphrody-labs/browser` imports and flattens upstreams (`tools/import.py`, `tools/flatten.py`, copies of obscura and agent-browser crates), which contradicts the no-copy rule. Either that repository becomes the engine workspace (lite engine and Blink build) consumed by `ay_browser` as a submodule, or it is retired in favour of submodules. Its owner decides. Evidence for the cost of copies, measured by the browser session on 2026-11-17 and not re-run here: `cargo check -p obscura-dom` on the build host fails with 4 errors at `aphrody-labs/browser` 7d2657d85, the same count as before its layout change (an earlier figure of 61 errors, inherited from a recovery note, is stale).
2. Whether `ay_browser` uses chromiumoxide's Browser/Page handler or only its CDP types (decided by the probe, step 1).
3. Whether upstream agent-browser accepts a `[lib]` split, and whether Phase B retires its daemon or keeps it.
4. Which CDP domains and DevTools features CEF 156 exposes (extensions, WebMCP): measured in step 1 and in the ADR-3 list.
5. WebDriver BiDi as a second transport (lightpanda serves it, Puppeteer and Playwright consume it): out of v1.

## Consequences
One verb vocabulary, one capability model and one registry replace five overlapping surfaces. Playwright keeps Firefox, WebKit, the test runner and codegen as an external dependency. The cost is the alias and registry maintenance against five fast-moving upstreams (agent-browser and Obscura release every few days to two weeks), which the registry check and the live-pin check are meant to catch.

## Blitz as a light backend (added 2026-11-16)
Blitz (DioxusLabs, Apache-2.0) renders no-JS pages in-process in Rust and ships an experimental CDP server (`blitz-vibey-cdp-server`). It is a candidate backend next to Lightpanda and Obscura, measured on the same pages. See `docs/research/dioxus-blitz.md`.
