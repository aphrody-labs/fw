# Kitesurf in this repository

aphrody-labs/kitesurf is the fork of [cloudflare/kitesurf](https://github.com/cloudflare/kitesurf) (Apache-2.0), pinned by commit in
`tools/layout.toml` [forks] (the build host fetches it; nothing is checked out here). It is an experimental browser engine in Rust and WebAssembly: Blitz for the
DOM and layout, Stylo for CSS, Taffy for flex and grid, Parley for text, Vello for painting, usvg for SVG and Boa for JavaScript, behind a Chrome
DevTools Protocol endpoint, with one isolate per page. It runs on Cloudflare Workers (Dynamic Workers, Workers Paid) and locally under `wrangler dev`.

## What the patches are

`scripts/patches/` of the fork holds the patches, one directory per vendored crate, applied to the crates.io sources. They are where Kitesurf's work is:

| Crate | What they add |
| :-- | :-- |
| blitz-dom | shadow DOM and slots, `::slotted`, flat-tree layout, CSS counters, anonymous table boxes, `:has()` invalidation, directionality and bidi, DOM selections and carets, incremental layout and damage tracking, lazy `@font-face`, form controls driven by the embedder |
| boa_engine, boa_parser, boa_ast, boa_gc, boa_interner | the JavaScript engine the pages run on |
| blitz-paint | painting of the new box kinds, re-anchored abspos, SVG fragments |
| stylo | relative-selector invalidation, animation state, the properties Gecko-only builds hide |
| parley, parley_engine | text layout |
| taffy | grid information, container size queries |
| usvg, jpeg-encoder, vello_cpu, vello_common, selectors, anyrender* | images, SVG with the embedder's fonts, CPU painting |
| playwright, puppeteer | the client patches that let the standard suites run |

## How close to Chrome it is

Kitesurf gates itself on the Web Platform Tests (a pinned corpus), the Puppeteer suite and the Playwright suite. Pass and failure counts per
test are in `tests/wpt-expectations.json`, `tests/pptr-expectations.json` and `tests/pw-expectations.json` of the pinned commit; they change with
every pin, so they are not copied here. No Chromium reference run of the same corpus exists yet (see "Not done").

## What this repository does with it

1. **Build and test on the build host only.** The Rust core's host tests are `bazel test //packages/core:host_tests`; nothing of Kitesurf is
   built on a workstation. `bun ../ay/tools/sync-host.ts` puts a checkout on the build host.
2. **Parity against the real thing (planned).** Run the WPT corpus pinned by Kitesurf (`tests/wpt-revision.json`) on the Chromium that `DEPS`
   pins (`headless_shell` from `build/engines`) to get reference expectations, then compare the two per area. The parity number of an engine
   is the share of the tests Chromium passes that it passes too, not the share of the corpus. `testing/acceptance.ts` does not do this yet.
3. **Work the failing areas by weight.** The largest failure counts first; each fix is a patch against
   the DioxusLabs crates, carried in the aphrody-labs forks of `blitz`, `taffy`, `anyrender` and `dioxus` rather than as vendored copies, and sent
   upstream when it is not Kitesurf-specific.
4. **Chromium itself.** Kitesurf is not a Chromium build and does not become one. Chromium is built from its own sources (`build/engines`)
   with the build tools of `ay-tools` (siso, ninja, gn, depot_tools at the revisions its DEPS pins); Kitesurf is the lite, edge and Rust path.

## State of the integration

- **Patches absorbed into forks.** Each patched crate was cut from an exact upstream commit (`.cargo_vcs_info.json` of the published crate); the series
  is applied there as one commit per patch, with its real author, on the `kitesurf` branch of the aphrody-labs fork: parley, blitz, anyrender, taffy,
  stylo (with selectors), boa, resvg (usvg), vello, ratatui-image, jpeg-encoder. Four of those forks were created for this. `tools/kitesurf-absorb.ts`
  does it (the Cargo.toml hunks, which target the normalized published manifest, are replayed on the upstream form).
- **One prefixed workspace.** The fork's `crates/` holds the 26 engine crates (the patched ones and the four that depend on them: glifo,
  stylo_dom, stylo_malloc_size_of, stylo_traits), named `browser-<crate>`, library names unchanged, siblings by path and version, `FEATURES.md` as the
  feature matrix. `tools/kitesurf-monorepo.ts` generates the layout from `cargo metadata` (the closure is computed, never listed). Nothing is published.
- **Bun.** `bun.lock` was migrated from `pnpm-lock.yaml` and the developer scripts run on Bun. Bazel still reads `pnpm-lock.yaml`, the patch series and the
  node-run tests (`e2e-global-setup`, the lint rule tests), so those stay until the Bazel rules are moved to the workspace and to Bun.
- **Not done.** Bazel on `crates/`; the Chromium WPT reference run and the per-area parity matrix; publishing.
