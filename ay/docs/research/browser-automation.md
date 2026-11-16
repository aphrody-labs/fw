# Browser automation: the projects, their overlaps and gaps

Retrieved 2026-11-16. Per-project rows and per-claim sources are in `browser-automation.toml`; anything not read from a source is written as unknown. This feeds ADR-7 (`../adr/0007-browser-automation.md`).

## Projects

| Project | Version (2026-11-16) | Licence | Language | Engine | Protocols |
| --- | --- | --- | --- | --- | --- |
| obscura | v0.2.7 (2026-11-13) | Apache-2.0 | Rust | own (V8 via deno_core, html5ever, taffy, tiny-skia) | CDP, MCP |
| lightpanda | 1.0.4 (2026-11-13) | AGPL-3.0-only | Zig (+ Rust FFI) | own (V8, html5ever, libcurl), no real rendering | CDP, WebDriver BiDi, MCP |
| agent-browser | v0.43.0 (2026-11-12) | Apache-2.0 | Rust (+ TS docs) | none: Chrome for Testing, lightpanda, obscura, Safari/iOS, cloud providers | CDP, WebDriver (Safari), MCP |
| playwright | v1.65.1 (2026-11-13) | Apache-2.0 | TypeScript | Chromium 159, Firefox 159, WebKit 27.2 (own patched builds) | CDP, own protocol, BiDi |
| playwright-mcp | 0.0.85 (2026-11-09) | Apache-2.0 | JavaScript | Playwright's | MCP |
| puppeteer | 25.17.0 (2026-11-10) | Apache-2.0 | TypeScript | Chrome, Firefox | CDP, BiDi |
| chrome-devtools-mcp | 1.13.0 (2026-11-11) | Apache-2.0 | TypeScript | Chrome and CfT only (official) | MCP over Puppeteer/CDP |
| chromiumoxide | 0.9.1 (2026-02-25) | MIT OR Apache-2.0 | Rust | Chrome/Chromium | CDP (typed, generated) |
| headless_chrome | 1.0.22 (2026-06-11) | MIT | Rust (sync) | Chrome/Chromium | CDP |
| Chrome for Testing + headless-shell | 157.0.8103.78 stable | tooling Apache-2.0; binary terms unknown | n/a | Chrome | CDP, chromedriver |
| CEF via cef-rs | cef 156.6.0+156.0.30 | Apache-2.0 OR MIT; CEF BSD-style | Rust over C API | Chromium (one milestone behind CfT) | CDP port, in-process DevTools API |

`aphrody-labs/browser` (Aphrody-owned, not a fork) already plans "one CDP front, two engines" (lite Obscura, full Blink) with a router; ADR-7 builds on that decision and does not redo its measurements.

## Feature matrix

Y = present in a source read; P = partial or only through raw CDP or another tool; - = not found in the sources read; ? = unknown. "-" means not found, not proven absent.

| Feature | obscura | lightpanda | agent-browser | playwright | playwright-mcp | puppeteer | devtools-mcp | chromiumoxide | headless_chrome | CfT | cef-rs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Snapshot with element refs | Y | P (tree, interactiveElements; no refs) | Y (@eN) | Y (ARIA snapshot, `snapshot`) | Y | - | Y (take_snapshot) | P (raw CDP) | P (raw CDP) | n/a | P (raw CDP) |
| Screenshot (raster) | Y (own paint) | P (text-only) | Y | Y | Y | Y | Y | P (raw CDP) | Y | Y | Y (OSR, ay_chromium) |
| PDF | Y (raster) | P (text-only) | Y | Y | Y (--caps=pdf) | Y | - | Y | Y | Y | ? |
| Network interception / mocking | Y (Fetch) | Y | Y (route) | Y | Y (--caps=network) | Y | Y (mock_network) | P (raw CDP) | Y | n/a | ? |
| Cookies and storage | Y | Y | Y | Y | Y (--caps=storage) | Y | - (skill only) | P | P | n/a | ? |
| Stealth / anti-fingerprint | Y | - | - | - | - | - | - | - | - | - | - |
| Multi-tab / isolated contexts | Y | Y (sessions, popups) | Y (tabs, sessions) | Y | Y | Y | Y (pages) | Y | Y (tabs, incognito) | n/a | P |
| Recording, trace, video | P (screencast) | P (PandaScript replay) | Y (record, trace, HAR, profiler) | Y (trace, video, HAR) | Y (--caps=devtools) | P (screencast; tracing class not read) | Y (perf trace, screencast, heap) | ? | P (JS coverage) | n/a | n/a |
| Codegen / script export | - | P (/save to PandaScript) | P (flow save) | Y (codegen, recorder) | P (generate_locator) | - | - | - | - | n/a | n/a |
| Test runner | - | - | - | Y | P (assertion tools) | - | - | - | - | n/a | n/a |
| Performance, Lighthouse, heap | - | - | P (vitals, profiler) | P (tracing) | - | - | Y | P (raw CDP) | P | n/a | n/a |
| Extensions, PWA, WebAuthn | - | - | P (extensions flag) | P (extension mode) | P (--extension) | - | Y | - | P (preload) | n/a | ? |
| WebMCP | - | - | Y | Y | - | Y (experimental) | Y | - | - | n/a | ? |
| MCP server (tools) | Y (14) | Y (40) | Y (160) | via playwright-mcp | Y (72) | - | Y (63) | - | - | - | - |
| CLI | Y | Y | Y (20 groups) | Y (test, codegen, agent CLI ~102 commands) | Y (server launcher) | P (browsers only) | Y (chrome-devtools) | - | - | - | - |
| Skills shipped | 1 | in binary + separate repo (not read) | 10 (versioned with CLI) | 3 | - (maintainer only) | 1 (maintainer) | 8 | - | - | - | - |
| In-process Rust API | Y (git crate) | - | - (bin only) | - | - | - | - | Y | Y | - | Y |
| Windows native | Y | - (WSL) | Y | Y | ? | ? | ? | Y (windows-registry dep) | Y (winreg dep) | Y (win64) | Y |
| Published perf numbers | vendor | vendor | - | - | - | - | - | - | - | - | - |

Published performance, with the sources and caveats in the toml:

- Lightpanda (vendor, 933 pages on m5.large): 123 MB vs 2 GB peak, 5 s vs 46 s per 100 pages.
- Obscura (vendor): 30 MB vs 200+ MB, 85 ms vs ~500 ms page load.
- Measured by Aphrody on fr1, one real page, median of 5 (`aphrody-labs/browser` ARCHITECTURE.md): headless-shell 578 MiB and 0.49 s; Obscura no-render 69 MiB and 0.66 s; Obscura render 116 MiB and 1.71 s. On a real page the lite engine wins on memory (8.4x) and not on time.

## Overlaps

1. **Tool vocabulary is implemented four to five times.** click, fill, type, press, hover, select, drag, upload, navigate, back, screenshot, evaluate, wait, tabs, dialogs, console, network list, cookies, storage, route, PDF exist in playwright-mcp (`browser_*`), agent-browser (`agent_browser_*`), DevTools MCP (snake_case), lightpanda (camelCase) and obscura (`browser_*`, a subset of playwright-mcp's names). Obscura already copied Playwright MCP's names; that is the de-facto convention.
2. **Snapshot-with-refs** is implemented by agent-browser, Playwright (ARIA snapshot), playwright-mcp, obscura and DevTools MCP with different ref syntax (`@e1`, `ref=e1`, uid).
3. **CDP engines and CDP clients** overlap: chromiumoxide, headless_chrome and Puppeteer each implement a CDP client; agent-browser contains a fourth (tokio-tungstenite). chromiumoxide_cdp already provides typed commands for the full protocol.
4. **Browser download and pinning** exists three times: `@puppeteer/browsers`, `chromiumoxide_fetcher`, agent-browser `install`; Playwright has its own CDN (`browsers.json`).
5. **Engine abstraction over CDP** is attempted by agent-browser (`--engine chrome|lightpanda|obscura`) and by `aphrody-labs/browser` (router lite to Blink); nobody else.
6. **Skills** are shipped per project in three styles: thin stub loading from the binary (agent-browser), in-tree skills (Playwright, DevTools MCP), none (most others).
7. **WebMCP** appears in four projects with four APIs.

## Gaps

1. **No capability model.** agent-browser errors on unsupported flags for lightpanda; no project lets an MCP client see which tools a backend supports. Lightpanda lacks extensions, profiles and storage state; obscura lacks BiDi and tracing; only Chromium has DevTools-class features.
2. **No Rust library with refs, tools and several backends.** The Rust pieces split: chromiumoxide and headless_chrome are bare CDP clients; agent-browser is a binary with no `[lib]`; obscura's crate is in-process but covers one engine and a git dependency; `ay_chromium` covers CEF only (fresh browser per call).
3. **Firefox and WebKit** only through Playwright's patched builds (and Puppeteer's Firefox BiDi). Lightpanda serves BiDi and Puppeteer speaks it, so a BiDi path exists but is untested here.
4. **Stealth** is only in obscura, and it is engine-bound (fingerprints from its own engine). Nothing offers stealth over Chromium.
5. **Test runner, codegen, trace viewer** are only in Playwright.
6. **Performance, Lighthouse, heap snapshot, extension, PWA, WebAuthn tooling** is only in DevTools MCP and needs Chrome/CfT.
7. **CEF is unserved**: no project drives CEF (DevTools MCP supports only Chrome/CfT officially); the `execute_dev_tools_method` route used by `ay_chromium` is unique to this repository.
8. **No independent benchmark across engines** beyond one page; agent-browser has a harness without results; vendor figures use different baselines.
9. **Licence mix**: lightpanda is AGPL-3.0-only, everything else is Apache-2.0 or MIT; process isolation over CDP keeps lightpanda out of linked code.
10. **Telemetry defaults**: lightpanda (on) and DevTools MCP (usage statistics, CrUX, update checks on) report by default; the ecosystem must set the opt-outs when it spawns or wraps them.

## Not sourced

Lightpanda WPT pass rate, lightpanda skill content and the `agent-skill` repo, obscura-benchmark results, agent-browser performance, Playwright performance, installed sizes of CEF and CfT, CfT chromedriver BiDi status, whether CEF 156 supports extensions and WebMCP, whether agent-browser or DevTools MCP attach to a CEF debugging port, Chrome binary licence terms, DevTools MCP's pinned puppeteer version.
