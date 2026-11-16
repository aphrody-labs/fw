# browser

A browser like Helium or Brave, full headless like Obscura, AI-ready like agent-browser, with Bun's performance and
layout. One Cargo workspace; Blink (Chromium headless, native CDP) renders, a lite engine (Obscura on
Servo's parsing and style crates) serves agents at a fraction of the memory.
Decision, measurements and engine plan: [ARCHITECTURE.md](ARCHITECTURE.md). Agent rules and sync commands: [AGENTS.md](AGENTS.md).

| Source | Where | History |
|---|---|---|
| Bun (aphrody-labs/bun, rebased on oven-sh/bun) | `src/*` as upstream | kept |
| Servo org: servo, stylo, webrender, mozjs, mozangle, html5ever, rust-url, rust-cssparser, ipc-channel, surfman, euclid, string-cache, unicode-bidi, core-foundation-rs, dwrote-rs, gleam, app_units, cgl-rs, fontsan, gaol, plane-split, rust-fnv, rust-freetype, rust-harfbuzz, smallbitvec, uluru | `src/<package>`, rest in `upstream/<repo>` | kept |
| Obscura (a user account, h4ckf0r0day/obscura, not an org) | `src/obscura-*` | kept |
| agent-browser (vercel-labs, aphrody-labs fork) | `src/agent-browser` | kept |
| lol-html (oven-sh, bun branch), rust-argon2 | `src/lol_html`, `src/rust-argon2` (Bun's vendored deps) | kept |

Skipped from the Servo org (listed with reason in [docs/inventory.md](docs/inventory.md)): archived repositories,
websites, CI and WPT dashboards, the wpt fork, demos, `tendril` (superseded by the copy inside html5ever),
`font-kit` and `pathfinder` (not in Servo's dependency graph), `malloc_size_of` (stylo carries its own).

```sh
bun tools/acceptance/cdp-probe.ts <chrome|headless_shell> <url> out.png --swiftshader   # CDP, WASM, WebGPU probe
python3 tools/bench/startup.py <url> 5 "name=cmd {url} {out}"                           # wall time and tree RSS
build/engines/build.sh                                                                  # V8 + Skia archive from the Chromium checkout
```
