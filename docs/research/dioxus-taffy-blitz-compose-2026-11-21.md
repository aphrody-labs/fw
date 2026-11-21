# Dioxus, Taffy, Blitz and Compose M3 audit (2026-11-21)

Deep-research workflow wf_26ef2b7e-08f: 103 agents, 3-vote adversarial verification per claim.

## Summary

As of 2026-11-21, the four stacks are at different maturity levels. Compose Material 3 1.5.0 (stable, Nov 18) with Compose 1.13 is the production-grade Android answer. Compose Multiplatform 1.13.0-rc01 is close behind, but its adaptive layouts are still alpha. Dioxus is stable at 0.7.10 while 0.8.0-alpha.3 rewrote its rendering core and still carries regressions. Blitz is officially beta and tracks Dioxus 0.8 alpha. Taffy is mature (v0.15.1 with Block, Flexbox and Grid, plus claimed speedups). Zed's GPUI pins Taffy 0.15.0 while Blitz pins a git rev, so the two share a layout engine but not a version. For this monorepo: keep GPUI for the desktop shell, since none of the audited alternatives replaces it. Use Compose M3 for Android. Treat Blitz as the candidate for HTML content rendering inside the desktop, but only behind a pinned beta. For web, Dioxus 0.7 stable is the only audited Rust-native option and the Compose web/wasm target is not clearly stable. Several of the user's requested dimensions (WPT pass rates, wasm bundle sizes, licences, Subsecond and Manganis, Taffy Au/fixed-point work, table and calc support) were not established by the verified claims and remain open.

## Verified findings

- **high** (3-0): Dioxus: stable is 0.7.10 (30 Jul 2026); 0.8.0-alpha.3 (16 Oct 2026) is the newest pre-release, after four alphas since alpha.0 on 19 May. No stable 0.8 yet.
  - https://github.com/DioxusLabs/dioxus/releases
- **high** (3-0): Dioxus 0.8 alpha.2 (#5554) rewrote core rendering: no comment-node placeholders at runtime or in hydration, rsx! expands to a const builder API, trait-based HTML elements moved out of dioxus_elements (breaking). Regressions followed (fragment sibling placement fixed in alpha.3 #5924; Native 'cannot find module html' #5927). Treat 0.8 alpha as unsuitable for production.
  - https://github.com/DioxusLabs/dioxus/releases
- **high** (3-0): Dioxus Native (Blitz renderer) tracks Blitz betas: 0.3.0-beta.1 in alpha.1, beta.3 in alpha.2, beta.4 bump in alpha.3. alpha.3 fixed NodeHandle methods (set_focus, scroll_to, get_client_rect) panicking with a RefCell double borrow by buffering mutations until after the VirtualDOM runs. The native path is therefore still shaking out crashes.
  - https://github.com/DioxusLabs/dioxus/releases
- **high** (3-0): Taffy consumers per its README: Servo, Blitz, Bevy, Takumi, iocraft, Slint, Floem (Lapce) and Zed GPUI. GPUI and Blitz both use Taffy but with different pins: Zed has taffy =0.15.0 from crates.io, Blitz pins a git rev.
  - https://github.com/dioxuslabs/taffy https://raw.githubusercontent.com/DioxusLabs/taffy/main/README.md https://docs.rs/crate/taffy/0.14.0
- **medium** (3-0 (README content); 0-3 (the 'only' framing)): Taffy's README states it implements Block, Flexbox and CSS Grid, with 'other paradigms planned'. Table layout is not listed and calc is not mentioned. Absence from the README is not proof of absence: a claim that Taffy implements ONLY these three was refuted 0-3 (calc support in particular needs checking in code).
  - https://raw.githubusercontent.com/DioxusLabs/taffy/main/README.md
- **medium** (3-0): Taffy performance: v0.15.1 (25 Oct 2026, latest) says flexbox is ~15-20% faster on deep trees (fewer allocations and passes over flex items); v0.15.0 (17 Oct 2026) says Grid ~60% faster and Flexbox ~15% faster, adds position: static and fixed with hoisting of absolute/fixed boxes, MSRV 1.71. Sticky positioning is not implemented (Position::Sticky is a stub laid out like static).
  - https://github-redirect.dependabot.com/DioxusLabs/taffy/releases
- **high** (3-0): Blitz is officially beta ('many no-JS sites render, but many bugs and missing features'). Architecture: blitz-dom uses Stylo (CSS), Taffy (box layout) and Parley (text); painting goes through blitz-paint to the separate anyrender abstraction, with backends anyrender_vello, vello_cpu, vello_gpu, skia and tiny_skia in the workspace manifest. The README's 'blitz-renderer-vello' crate no longer exists (stale README), so do not cite it. Skia is a first-class backend, though the README never mentions it.
  - https://github.com/DioxusLabs/blitz https://raw.githubusercontent.com/DioxusLabs/blitz/main/README.md
- **high** (3-0): Git Blitz/Dioxus Native depends on Dioxus 0.8.0-alpha.3 (commits before 286874c, 2026-10-23, use 0.7.x). The README gives no WPT pass rates, only a link to blitz.is/status/css. Quantified CSS coverage was therefore not obtained.
  - https://github.com/DioxusLabs/blitz
- **high** (3-0): Compose Material 3 1.5.0 is stable (18 Nov 2026), following beta01 (7 Oct), beta02 (21 Oct) and rc01 (4 Nov); 1.6.0-alpha01 shipped the same day. Jetpack Compose November '26 is 1.13 (BOM 2026.11.01) and requires Kotlin 2.2+ and compileSdk 37.1.
  - https://developer.android.com/jetpack/androidx/releases/compose-material3 https://developer.android.com/blog/posts/jetpack-compose-november-2026-release
- **high** (3-0): Material 3 Expressive: theming graduated in 1.5.0 (MaterialTheme.expressiveLightColorScheme, materialExpressTheme); ButtonGroup went stable and FloatingToolbar went non-experimental in 1.5.0-alpha22 (17 Jun 2026); ToggleButtons, WavyProgressIndicators and TimePickers graduated; LoadingIndicator promotion was reverted in alpha19 and it remains experimental. Experimental annotations were cut from 120 to 58 (52%), and Material 2 is deprecated.
  - https://developer.android.com/jetpack/androidx/releases/compose-material3 https://developer.android.com/blog/posts/jetpack-compose-november-2026-release
- **high** (3-0): Adaptive layouts: Material 3 Adaptive stable is 1.3.0 (12 Aug 2026), which added adaptive-navigation3 with ListDetailSceneStrategy and SupportingPaneSceneStrategy for Navigation 3. The only pre-release is 1.4.0-alpha03 (page updated 7 Oct 2026; no beta or RC on Google Maven).
  - https://developer.android.com/jetpack/androidx/releases/compose-material3-adaptive
- **high** (3-0): Google's hero benchmarks on a Pixel 6 (Pokedex app, Android 17, R8, locked clocks): Compose 1.13 TTID 147.65 ms vs Views 152.87 ms; TTFD Views faster (180.32 vs 202.79 ms); scroll P50 frame duration 3.51 ms (Compose) vs 3.06 ms (Views); 0% jank. The Compose edge in TTID is about 5 ms and Views stay ahead on TTFD and scroll. These are first-party numbers on one device and one app.
  - https://developer.android.com/blog/posts/jetpack-compose-november-2026-release https://developer.android.com/develop/ui/compose/performance/herobenchmark
- **medium** (2-1): Compose Multiplatform 1.13.0-rc01 (18 Nov 2026): Material3 is based on Jetpack Material3 1.5.0-rc01 and Material3 Adaptive is 1.4.0-alpha01 (Jetpack Adaptive 1.4.0-alpha03), so adaptive layouts on CMP are alpha-level in this line. Caveat: this is an RC, not a final 1.13.0, and the 'alpha-level' wording is an inference.
  - https://kotlinlang.org/docs/multiplatform/whats-new-compose-113.html
- **medium** (3-0): Compose for web (wasm/canvas) in CMP 1.13: ComposeViewportConfiguration and the resource preload API became Stable with no opt-in; WebGL texture interop (WebGLRenderTarget) is new and Experimental. The page gives no explicit Beta/Stable label for the web target as a whole (the Sept 2025 CMP 1.9.0 blog put web in Beta; no source in this audit says Stable).
  - https://kotlinlang.org/docs/multiplatform/whats-new-compose-113.html

## caveats

Coverage is uneven and the matrix is incomplete. Verified claims cover release states (Dioxus, Taffy, Blitz, Compose M3, Compose Multiplatform) well, but several requested items were NOT established and should not be filled from memory: Dioxus wasm bundle sizes and measured runtime performance, Subsecond/Manganis/wasm-split/fullstack details, production users, and the Dioxus licence; Taffy Au/fixed-point work, known panics, conformance suites, and the Taffy versions pinned by Servo, Bevy, Slint and Floem; Blitz WPT pass rates, Skia-vs-Vello defaults, accessibility, and the effort of embedding Blitz inside GPUI; licences for all four stacks; how Now in Android structures its design system; and any head-to-head benchmark against GPUI (GPUI itself was barely covered). The Taffy performance figures (15-20% flexbox, 60% grid) are maintainer self-reports without methodology, and the Compose benchmarks are Google first-party on one Pixel 6 with one app. Three Taffy claims were refuted 0-3, including the claim that Taffy lacks table and calc and the Yoga benchmark numbers; do not cite them. The Blitz README is stale (names the nonexistent blitz-renderer-vello crate and omits Skia and WPT numbers). Everything is time-sensitive: Dioxus 0.8 is alpha, Blitz is 0.3.0-beta, CMP is at 1.13.0-rc01, Material3 Adaptive on CMP is alpha, and Compose M3 1.6.0-alpha01 landed the same day as 1.5.0. Recommendations by surface are inferred from these facts, not directly verified: desktop shell stays GPUI; Android uses Compose M3 1.5.0 (Compose 1.13, Kotlin 2.2+, compileSdk 37.1); HTML content rendering inside the desktop is a Blitz candidate behind a pinned beta with a fallback such as an embedded browser; web has Dioxus 0.7.10 (Rust-native) or Compose web (target maturity not labelled Stable in the audited source).

## openQuestions

```json
[
 "What are Blitz's current WPT/CSS pass rates (blitz.is/status/css) and its Skia-vs-Vello default per platform, and can blitz-dom/anyrender render into a GPUI-owned wgpu surface or texture, including input, accessibility and text-selection integration?",
 "What are measured wasm bundle sizes and runtime costs for Dioxus 0.7.10 web (with and without wasm-split) and for Compose Multiplatform web/wasm, and is the CMP web target still Beta as of 1.13?",
 "Which Taffy versions do Servo, Bevy, Slint and Floem actually pin against GPUI's =0.15.0, and does Taffy have table or calc support in code beyond the README, along with the fixed-point (Au) and relayout work and known panics?",
 "What licences apply (Dioxus, Taffy, Blitz, Compose, Compose Multiplatform, Stylo/Parley/Vello/Skia transitive), and how does Now in Android structure its core/designsystem module on the 1.5.0 Expressive APIs?"
]
```

## Measured in fw main by the coordinator

- taffy 0.15.1 is used by gpui, aphrody_session, nsh-core and ay-design.
- dioxus 0.7.10 is used only by ay-design.
- `crates/fw-ui-blitz` depends only on flate2 and serde: it is not Blitz. It is purged.
