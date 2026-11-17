# Chromium stack for the browser product

Retrieved 2026-11-16. Data and per-row sources are in `chromium-stack.toml`. Anything not read from a source is written as unknown.

## Summary table

| Component | Version (2026-11-16) | Licence | Fetched by | Rust story |
| --- | --- | --- | --- | --- |
| Chromium | main 160.0.8167.0; CfT stable 157.0.8103.78 | BSD-style, Chromium Authors | depot_tools `fetch`, gclient, `autoninja` (Siso) | `rust_static_library`, cxx, Crubit, Mojo; no Rust UI |
| CEF | stable 156.0.30 (Chromium 156.0.8078.96); beta 158.0.3 | BSD (GitHub says NOASSERTION) | prebuilt from cef-builds.spotifycdn.com | none inside; C API |
| cef-rs | cef 156.6.0+156.0.30, download-cef 3.0.1 | Apache-2.0 OR MIT | crates.io; local fork at 156.4.0, behind upstream | bindgen over the C API, safe wrapper, osr example on winit and wgpu |
| Chrome for Testing | stable 157.0.8103.78, rev 1717349 | tooling Apache-2.0; binaries not read | JSON endpoints, zips on storage.googleapis.com | none |
| chrome-headless-shell | 157.0.8103.78 | as CfT | CfT zip | none |
| Puppeteer | 25.17.0 (2026-11-10) | Apache-2.0 | npm, node >=22.12 | none |
| Chrome DevTools MCP | 1.13.0 (2026-11-11) | Apache-2.0 | npm | none |
| V8 | main 16.0.0.0 | BSD-style | Chromium DEPS | C++; rusty_v8 for embedding |
| Skia | milestone 160 | BSD-3-Clause | Chromium DEPS | C++ |
| Blink | part of Chromium | unknown (no file read) | Chromium checkout | C++ |
| Dawn | no releases; DEPS pin 8f21d51 | BSD-style | Chromium DEPS | C++; Rust equivalent is wgpu v30.0.2 |
| depot_tools | pin ec77017, Siso pin 3025be9 | BSD-style | git clone | Python |
| Views, gfx, Ozone, chrome/browser/ui | part of Chromium | Chromium | Chromium checkout | C++ only |
| content_shell | part of Chromium | Chromium | Chromium checkout | C++ |

Facts that shape the decision:

- Linux build floor: 8 GB RAM minimum (16 GB or more recommended), 100 GB free disk, clang with libc++ only. Windows needs 100 GB on NTFS and Visual Studio 2026. Fetch takes about 30 minutes on a fast link; compile time is not stated in the docs read.
- The build host's root filesystem has ~50 GB free, but its build volume `$WORK` (XFS on LVM, 805 GB, also NFS-mounted on fr2 at $WORK/shared) has ~740 GB free (`bun tools/disks.ts`, 2026-11-16). Disk is not a blocker.
- Chrome stable moves every 14 to 21 days (M156 2026-10-20, M157 2026-11-03, M158 2026-11-17, M159 2026-12-08). CEF stable is at 156 while Chrome for Testing is at 157.
- Prebuilt sizes (archives): CfT `chrome-linux64.zip` 203,266,906 B, `chrome-headless-shell-linux64.zip` 127,965,683 B, CEF linux64 `minimal` 333,332,290 B and `standard` 697,434,558 B. Installed sizes are unknown.
- Chrome DevTools MCP officially supports only Chrome and Chrome for Testing; usage statistics and CrUX lookups are on by default.
- Chromium's Ozone platforms in tree: cast, drm, flatland, headless, wayland, x11. Browser UI lives in `chrome/browser/ui/views` (329 entries) on the Views toolkit, which is C++.
- Rust in Chromium is for libraries and services; the docs name Mojo as the way to avoid direct C++ interop. A Rust UI cannot replace Views.
- WebMCP is a spec repository (webmachinelearning/webmcp); whether any Chromium or CEF build ships it is unknown.

## Three ways to build the browser

| | (a) Chromium fork, our UI in Views | (b) CEF and cef-rs, native GPUI UI | (c) content_shell based |
| --- | --- | --- | --- |
| Effort | Highest: C++ UI, own branding, own update channel, full Chromium CI | Medium: Rust shell for tabs, omnibox and agent panel; GPUI to CEF embedding unproven | High for little value: tabs, profiles, history, sync and extensions all ours |
| What we control | Everything: omnibox, tabs, default search, agent panel, flags | The CEF API: network, cookies, requests, DevTools port, off-screen frames; internal Chromium UI features only if CEF exposes them (unknown) | Content layer only, no browser features |
| Update cost | Highest: rebase every 14 to 21 days, touching `chrome/browser/ui`; we track security fixes | Low: bump the crate and the CEF build; about one milestone behind stable | High: content API churn, no stability guarantee found in the docs read |
| Binary size | Unknown until built (CfT chrome zip 203 MB as a hint) | CEF archive 333 MB minimal, 697 MB standard; installed size unknown | Unknown until built |
| Rust fit | Weak: libraries, Mojo, cxx only | Strong: `cef` crate on crates.io, osr example on winit and wgpu | Weak |
| Blocker | 100 GB disk (available on $WORK) | None for prebuilt | 100 GB disk |

## Recommendation

Choose (b), CEF with cef-rs and our GPUI shell, because it is the same engine without a Chromium build, has the lowest update cost and is the only option with a maintained Rust crate; keep the engine interface so (a) stays reachable. Use Chrome for Testing and chrome-headless-shell for tests and agents (craft-anything captures, claude-in-chrome, DevTools MCP), since DevTools MCP supports them officially.

Measurements still needed to accept ADR-3:

1. Prototype cef-rs inside GPUI on Linux (Wayland and X11) and Windows; record input latency, frame time and memory per tab, for both off-screen and native-window embedding.
2. Installed size of CEF minimal and standard, and the full shipped size with the shell, against CfT.
3. Feature probe on CEF 156 and 158: CDP attach from DevTools MCP and Puppeteer, extensions, WebGPU, and any WebMCP support.
4. web-platform-tests pass rate on CfT 157 as the upper bound, then on the CEF build.
5. CEF update lag in days behind Chrome stable over several milestones.
6. Only if (a) or (c) stays alive: a build directory on fr1 $WORK, then fetch time, `autoninja` release time with ThinLTO, peak RAM, disk use and binary size, with content_shell measured next to chrome.

Not sourced: Chromium compile time, CEF source-build time and requirements, installed sizes, Blink's licence, Chrome binary licence terms, rusty_v8 and rust-skia versions, whether WebMCP ships in Chromium or CEF, whether cef-rs accelerated off-screen rendering works on each OS.
