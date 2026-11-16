# ADR-3: Browser on CEF with cef-rs and the native UI from aphrody-dev/aphrody

Status: proposed, 2026-11-16; accepted after the measurements below. The audit is in `docs/research/chromium-stack.md`.

## Decision (proposed)
- **Engine**: Chromium through CEF (chromiumembedded/cef), driven from Rust with cef-rs (tauri-apps/cef-rs, fork in
  aphrody-labs). It is the same Blink, V8, Skia and Dawn as Chrome, with no Chromium build of our own. The update
  cost is a CEF bump, and it is the only option with a maintained Rust crate.
- **UI**: our own chrome (tabs, omnibox with AI mode, agent panel, WebMCP tools) is native UI owned by
  `aphrody-dev/aphrody` (GPUI), embedding CEF's off-screen or windowed rendering.
- **One engine interface** keeps a full Chromium fork with Views (option a) reachable if CEF limits us.
- **Testing and agents**: Chrome for Testing and chrome-headless-shell, which Puppeteer and Chrome DevTools MCP
  support officially; craft-anything captures pages through CDP on them.

## Options compared (from the audit)
| Option | Build cost | Control | Update cost | Rust fit |
|---|---|---|---|---|
| (a) Chromium fork, UI in Views | ≥100 GB disk, full depot_tools build | total | rebase every milestone | Views is C++ only; Rust only as libraries or Mojo services |
| (b) CEF + cef-rs + GPUI shell | prebuilt CEF binaries | engine as shipped by CEF | CEF version bump | maintained crate |
| (c) content_shell base | full Chromium build | low-level, no browser features | rebase every milestone | as (a) |

## Measurements required before acceptance
- cef-rs off-screen rendering embedded in a GPUI window, on Linux (Wayland) and Windows.
- Binary size and memory of a CEF build with our shell.
- web-platform-tests pass rate on the CEF build.
- Whether WebMCP ships in Chromium/CEF or needs our own bridge (the audit found only the spec repository).

## Known facts and lag (2026-11-16)
CEF stable 156.0.30 (Chromium 156) while Chrome for Testing stable is 157.0.8103.78. The aphrody-labs cef-rs fork
is at 156.4.0, behind upstream 156.6.0. A Chromium build (option a) needs ≥100 GB free; the build host has 47 GB.
