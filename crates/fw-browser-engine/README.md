# browser

A browser like Helium or Brave, full headless like Obscura, AI-ready like agent-browser, with Bun's performance and
layout. One Cargo workspace; Blink (Chromium headless, native CDP) renders, a lite engine (Obscura on
Servo's parsing and style crates) serves agents at a fraction of the memory.
Decision, measurements and engine plan: [ARCHITECTURE.md](ARCHITECTURE.md). Agent rules and sync commands: [AGENTS.md](AGENTS.md).

| Source | Where | History |
|---|---|---|
| Bun (aphrody-labs/bun, rebased on oven-sh/bun) | submodule `third_party/bun` | its own |
| Servo org: servo, stylo, webrender, mozjs, mozangle, html5ever, rust-url, rust-cssparser, ipc-channel, surfman, euclid, string-cache, unicode-bidi, core-foundation-rs, dwrote-rs, gleam, app_units, cgl-rs, fontsan, gaol, plane-split, rust-fnv, rust-freetype, rust-harfbuzz, smallbitvec, uluru | `third_party/<repo>/<package>`, the rest of the repository beside them | kept |
| Obscura (a user account, h4ckf0r0day/obscura, not an org) | `third_party/obscura/obscura-*` | kept |
| agent-browser (vercel-labs, aphrody-labs fork) | `third_party/agent-browser/agent-browser` | kept |
| lol-html (oven-sh, bun branch), rust-argon2 | `third_party/lol-html/lol_html`, `third_party/rust-argon2/rust-argon2` (Bun's vendored deps) | kept |

Skipped from the Servo org (listed with the reason by `bun tools/inventory.ts` into the private `.local/docs/`): archived repositories,
websites, CI and WPT dashboards, the wpt fork, demos, `tendril` (superseded by the copy inside html5ever),
`font-kit` and `pathfinder` (not in Servo's dependency graph), `malloc_size_of` (stylo carries its own).

```sh
bun PRESUBMIT.ts                                # types, lint, format, tests, layout, Bun-native proof
bun testing/acceptance.ts run                      # every engine x the live Google/Alphabet corpus: verdicts, screenshots, report
bun testing/acceptance.ts bench <url>              # hyperfine cold start + process-tree RSS; --jetstream for JetStream
bun testing/acceptance.ts track                    # Chromium, Servo, Blitz, Obscura, SpiderMonkey, WebKit ... against our pins
bun testing/acceptance.ts serve --schedule         # live dashboard (HTML, JSON, SSE) and its cron
build/engines/build.sh                           # V8 + Skia archive from the Chromium checkout
```
