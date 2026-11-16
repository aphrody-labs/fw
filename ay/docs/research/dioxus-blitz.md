# Dioxus, Blitz and Taffy

Read live on 2026-11-16 (gh api, crates.io). All repos below are forked or synced in aphrody-labs.

| Project | What it is | Version (crates.io) | Licence | Stars |
|---|---|---|---|---|
| taffy | Flexbox, grid and block layout in Rust; used by Blitz, Dioxus and GPUI | 0.15.1 (2026-10-25), 20.4 M downloads | MIT | 3.8k |
| blitz | A modular HTML/CSS rendering engine without a JS engine by default; no-JS sites (Wikipedia, old Reddit) render, beta | 0.3.0-beta.4 (2026-10-16) | Apache-2.0 / MIT | 4.5k |
| dioxus | A Rust UI framework for web, desktop, mobile and server, with fullstack and a deploy service (dioxuslabs.com/deploy) | stable v0.7.10 (2026-07-30); 0.8.0-alpha.3 on crates.io | Apache-2.0 / MIT | 39.8k |
| dioxus-native | Dioxus apps rendered by Blitz instead of a webview | 0.8.0-alpha.3 | Apache-2.0 / MIT | in dioxus |
| dioxus-components | A component library for Dioxus | no release | Apache-2.0 | 356 |
| sledgehammer_bindgen | A fast JS binding generator used by Dioxus's web renderer | 0.6.0 (2024-08-22) | none stated | 455 |
| awesome-dioxus, docsite, blitz-website | Ecosystem list, docs site, blitz.is site | none | none stated | — |

## How Blitz is built
- **Packages:** blitz-dom, blitz-html (html5ever), blitz-net, blitz-paint, blitz-shell (winit window), blitz-traits, stylo_taffy, accesskit_xplat, dioxus-native and dioxus-native-dom.
- **Ecosystem stack:** Stylo for CSS (crate `stylo` 0.24.0, from Servo), Taffy for layout, Parley for text, Vello or AnyRender for painting (`vello` 0.12.0, `anyrender` 0.15.0) and AccessKit for accessibility.
- **Experimental:** `blitz-vibey-script` runs JavaScript with the Boa engine; `blitz-vibey-cdp-server` is a Chrome DevTools Protocol server.
- **Status:** a `wpt/` runner tracks web-platform-tests; blitz.is publishes CSS status pages and builds for Windows, macOS, Linux and Android.

## What it means for Aphrody
- **Browser automation (ADR-7).** Blitz is a candidate light backend for `ay_browser`: no-JS pages rendered in-process, in Rust, Apache-2.0. That is the use case Lightpanda (AGPL, spawned) and Obscura cover. Its CDP server would let agent-browser and the other CDP clients drive it. It is measured next to Lightpanda and Obscura on the same pages.
- **Browser engine (ADR-3).** Blitz and Servo are the two Rust engines. Blitz is not a full browser: no JS by default, no WebRTC or storage, by design. It suits embedded HTML views (docs, mail, results pages) more than general browsing, where CEF stays.
- **Search and Craft.** blitz-html and blitz-dom (html5ever, Stylo, Taffy) can lay out and paint a page without Chromium. That serves server-side page snapshots and craft's no-JS captures.
- **Native UI.** Dioxus and dioxus-native are an alternative to GPUI for the native shell. Native UI is owned by aphrody-dev/aphrody, so the choice is recorded there. Taffy is already shared with GPUI.
- **Web front ends.** Dioxus fullstack is a Rust option for `apps/search` and `apps/cloud`, with Bun fullstack as the alternative. It is listed in `ui-frameworks.toml` for the comparison.
