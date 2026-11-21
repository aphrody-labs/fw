# Servo, Brave and Helium compared

Read live on 2026-11-16 with `bun tools/recon.ts` (GitHub orgs, site sitemaps) and `gh api` (repository contents,
releases). Raw data: `$AY_DATA_DIR/recon/{servo,brave,imputnet}-org/*.json`.

| | Servo | Brave | Helium |
|---|---|---|---|
| Organisation | `servo` (192 repos, 48 active) | `brave` (260 repos, 76 active) | `imputnet` (29 repos, 21 active) |
| What it is | An independent web engine in Rust, embeddable | A Chromium-based browser with its own features | A Chromium-based browser built on ungoogled-chromium |
| Engine | Its own (Rust): `components/` script, layout, net, paint, fonts, canvas, webgpu, webxr, devtools, webdriver_server, … | Chromium (Blink, V8) | Chromium (Blink, V8) |
| Latest release | v0.8.0 (2026-11-09); crate `servo` 0.8.0 on crates.io (2026-11-07) | v1.99.57 (2026-11-12) | 0.22.0 (2026-11-16), on Chromium 158.0.8127.36 |
| Main repos | `servo` ★38.5k, webrender ★3.4k, html5ever ★2.7k, rust-url ★1.6k, rust-cssparser, font-kit, euclid, ipc-channel, mozjs | `brave-browser` ★24.2k (build entry), `brave-core` ★3.8k (C++ on Chromium), adblock-rust ★2.9k, brave-search-mcp-server ★1.6k, go-sync | `helium` ★22.8k, helium-windows, helium-macos, helium-linux, helium-services, helium-onboarding |
| How the browser is made | One Cargo workspace (about 385 entries in the root manifest), crates published on crates.io | Chromium checkout plus `brave-core` mounted at `src/brave`: about 1000+ patch files in `patches/`, `chromium_src/` overrides, `DEPS` pulls prebuilt tools (node, ast-grep, rust-toolchain, updater, WireGuard DLLs) | A patch series of 359 patches applied to Chromium: 269 Helium, 70 ungoogled-chromium, 6 inox, 4 Brave, 3 Bromite, 3 Iridium, 3 upstream fixes, 1 Debian; plus pruning and domain-substitution lists |
| Rust | Everything | adblock-rust and Rust crates inside brave-core | Chromium's own Rust only |
| Licence | MPL-2.0 | MPL-2.0 | GPL-3.0 (plus ungoogled-chromium licence) |
| Website | servo.org (sitemap not reachable from this host on 2026-11-16) | brave.com: 3,573 sitemap URLs | helium.computer: no sitemap |
| Ecosystem we can reuse | html5ever, cssparser, rust-url, webrender, font-kit, euclid, ipc-channel: crates for our own Rust code; `servo` as an embeddable engine to evaluate behind the ADR-3 engine interface | adblock-rust (content blocking), the Brave Search MCP server pattern, the patch-based fork model | the ungoogled patch set and the de-googling and pruning lists; a patch-series fork of Chromium kept current (158 while Chrome for Testing stable is 157) |

## What it means for Aphrody Browser (ADR-3)
- **Chromium route** (CEF today): Helium and Brave show the two ways to keep a Chromium-based browser current.
  Helium uses a patch series with pruning lists (small team, fast rebase). Brave uses a patches directory plus `chromium_src`
  overrides (large team, deep changes). Our CEF + cef-rs route avoids both until we need engine-level changes; if
  we do, Helium's patch-series model is the lighter one to adopt.
- **Rust engine route**: Servo is the only one of the three that is a Rust engine and is published as a crate
  (`servo` 0.8.0). It belongs in the ADR-3 measurements as a second backend behind the engine interface:
  web-platform-tests pass rate, embedding in the native UI, size.
- **Reusable now**:
  - adblock-rust for content blocking in the browser and in the crawler;
  - html5ever and cssparser for parsing in search and craft;
  - the ungoogled pruning and domain lists for privacy defaults.

## Not sourced
- Servo's website sitemap (unreachable from this host).
- Brave's exact patch count (the contents API caps at 1000 entries).
- Per-browser benchmarks: none are compared here, because none were measured on the same machine.
