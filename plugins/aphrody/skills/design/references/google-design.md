<!-- Merged from skill `google-design` into `design`. Original frontmatter:
name: google-design
description: Authoritative design system specification anchored to https://design.google/ (Material 3 Expressive, Neural Expressive Nov 2026, Code is a Design Material, Google Sans Flex) across the entire Aphrody monorepo, GPUI, egui_gpui, bevy_gpui, aphrody_craft, gpui-kit, NSH, Stitch Studio, and Wasm/WebGPU applications.
-->
# google-design

> Authoritative design system specification anchored to https://design.google/ (Material 3 Expressive, Neural Expressive Nov 2026, Code is a Design Material, Google Sans Flex) across the entire Aphrody monorepo, GPUI, egui_gpui, bevy_gpui, aphrody_craft, gpui-kit, NSH, Stitch Studio, and Wasm/WebGPU applications.


# Google Design Source of Truth (`https://design.google/`)

Every UI surface, theme, component token table, shader palette, and studio workspace in Aphrody derives from **`https://design.google/`** and the canonical [`docs/design/`](../../../../docs/design/manifest.json) package:

- **Material 3 Expressive & Neural Expressive (Nov 2026)** 3-tier tokens (`assets/os/google_design_tokens.toml`, [`docs/design/stylesheet.css`](../../../../docs/design/stylesheet.css), `aphrody_ui::google_design`)
- [*Code is a Design Material*](https://design.google/library/code-is-a-design-material): live Rust/WGSL/TypeScript code + shaders as expressive design material
- [*Google Sans Flex*](https://design.google/library/google-sans-flex-font): 6 variable axes (`opsz`, `wght`, `wdth`, `GRAD`, `slnt`, `ROND`) paired with `JetBrains Mono` for code & hex telemetry
- Root [`DESIGN.md`](../../../../DESIGN.md), [`docs/design/DESIGN.md`](../../../../docs/design/DESIGN.md), Figma W3C DTCG kit ([`docs/design/kits/figma-design-kit.tokens.json`](../../../../docs/design/kits/figma-design-kit.tokens.json)), and Google Stitch kit ([`docs/design/kits/stitch-design-kit.json`](../../../../docs/design/kits/stitch-design-kit.json))
- Dual-GUI & 3D Studio Stack ([`references/google-design/references/gpui-and-egui-components.md`](references/google-design/references/gpui-and-egui-components.md)):
  - **GPUI Retained Mode**: `aphrody_ui::google_design_components` (`ExpressiveOmnibar`, `CascadingPopoverCard`, `GroundingSourceCard`, `SegmentedStudioBar`, `ThinkingAuraBadge`) + `aphrody_ui::starter_pack`
  - **Immediate-Mode `egui 0.37` Bridge**: `egui_gpui` (`google_design_to_egui_visuals`, `CraftDockTabs`, `CraftComboMenu`, `CraftToolbarChip`, `CraftPropertyRow`, `CraftImeBanner`)
  - **Unreal Bevy 5.8 Viewport & MaterialX**: `bevy_gpui` (Nanite 64v/124t meshlets, OpenPBR/MaterialX -> Naga v30 WGSL, Solari GI, OpenUSD Crate LZ4)
  - **Shared Studio Core**: `aphrody_craft` (Google FigCraft + Google Craft Anything + Unreal Bevy 5.8 + 18 Storytold Craft Apps in `GpuiGoogleDesign`, `EguiImmediate`, or `SplitSideBySide` parity mode)

## Spatial vs Effects Physics Springs

- **Spatial springs** (`damping_ratio = 0.9`): size, layout, position, and shape morphing (`spatial_fast = 1400`, `spatial_default = 700`, `spatial_slow = 300`).
- **Effects springs** (`damping_ratio = 1.0`): opacity, color, and elevation transitions (`effects_fast = 3800`, `effects_default = 1600`, `effects_slow = 800`).

## Automation & Native Commands

- Build & validate complete `docs/design/` asset/kit/MDX/schema package: `bun run design:pack`
- Sync tokens, sitemap snapshot & `DESIGN.md`: `bun run design:sync`
- Audit monorepo & templates: `bun run design:audit`
- Lint any `DESIGN.md`: `cargo run -p aphrody_cli -- design design-md lint docs/design/DESIGN.md`
- Open Studio & Parity workspaces inside Aphrody IDE (`crates/aphrody_nsh/src/design_stitch_studio.rs`):
  - `aphrody_nsh::OpenGoogleDesignMirror`
  - `aphrody_nsh::OpenStitchStudio`
  - `aphrody_nsh::OpenGpuiStarterPack`
  - `aphrody_nsh::OpenGoogleFigCraftStudio`
  - `aphrody_nsh::OpenGoogleCraftAnythingStudio`
  - `aphrody_nsh::OpenUnrealBevyStudio`
  - `aphrody_nsh::ToggleStudioGuiParity`

## Googlebook product page, measured (2026-11-21)

Reference: `https://googlebook.google/intl/fr_fr/`, build `googlebook-0.1.31-5b0f305`. Measured in Chrome (claude-in-chrome, 1994 CSS px wide at DPR 1.25) and in the Browser pane emulating 375x812 at DPR 2: Resource Timing, `HEAD` and `curl` for byte weights and headers, `ffprobe` on two files. Google's assets and text are a reference, never content: nothing below is copied into a product. The matrix that maps each pattern to our equivalent is `docs/product/cahier-des-charges.md`.

### Document

| Fact | Value |
|---|---|
| HTML | 58,789 B encoded, 369,964 B decoded, `Cache-Control: no-cache, must-revalidate`, HTTP/3 (`Alt-Svc: h3`), `<html lang="fr">` |
| Inline SVG sprite | 90 `<symbol>`, 84,030 characters inside the HTML (22.7 % of the decoded document) |
| Metadata | one JSON-LD `WebPage`; `og:title url image(:url :width 1440 :height 700 :type image/webp) description locale site_name type`; `twitter:card summary_large_image` with title, description, url; canonical; favicon.ico 5,617 B |
| Connections | `preconnect` to fonts.googleapis.com, fonts.gstatic.com, www.gstatic.com |
| Page height | 22,868 px at 1994 wide, 18,145 px at 375; header 80 px, footer 1,037 px, 28 modules in `#page-content` |

### Scripts, styles, fonts

| File | Raw | Encoded | `Cache-Control` |
|---|---|---|---|
| `All.min.css` | 725,075 B | 85,187 B gzip; `Accept-Encoding: br` alone gets it uncompressed | `public, max-age=3000` |
| `All.min.js` | 44,054 B | 12,106 B br | `public, max-age=3000` |
| `cookienotificationbar.min.css` / `.js` | 9,139 B / 79,721 B | not compressed by `curl` with br | `private, max-age=3000` |
| `gtm.js` | 1,624 B | 551 B br | `public, max-age=3000` |
| Google Fonts `css2` (Product Sans, Google Sans Display, Google Sans, Google Sans Text, `display=swap`) | 151,980 B | 5,119 B | n/a |

- Body text is Google Sans 16 px, white on `rgb(0, 0, 0)`; a section title is Google Sans 48 px / 56 px, weight 500, letter-spacing -0.5 px. The variable Google Sans Flex is not used.
- Breakpoints in `All.min.css` (rule counts): `min-width: 1024px` 405, `min-width: 600px` 389, `min-width: 1440px` 114, `max-width: 599px` 63, 600 to 1023 19, 1024 to 1439 8, `min-width: 1620px` 7, `min-width: 768px` 2. Layout steps are 600, 1024 and 1440.
- `forced-colors: active` (with `-ms-high-contrast`) 137 blocks; `prefers-reduced-motion` 24 blocks.
- Tokens: `--md-sys-*` referenced 222 times, `--md-ref-*` 98 times (Material 3 role names).
- Easing curves by use count: `cubic-bezier(.4,0,1,1)` 42, `(0,0,.2,1)` 40, `(.4,0,.2,1)` 33, `(.22,1,.36,1)` 9, `(.45,0,.06,1)` 2. Typical transition 150 ms (leave) and 200 ms (enter) on `transform` and `opacity`.
- Aspect ratios declared: `1`, `16/9`, `2/1`, `1.6`, `1.5`, `4/3`. 10 `position: sticky`, `scroll-snap-type` x and y mandatory (3 each), one scroll-driven animation (`view-timeline-name: --pinned-promo-timeline`, `animation-timeline: view()`).

### Page structure (module classes of `#page-content`, top to bottom)

1. `gb-scroll-scrub-video-hero`, 2,136 px tall with a pinned 1,068 px inner stage: one ambient video, a logo and a tagline.
2. `card-list-a` (3 images), then `page-promo-c` (`smarter-way-to-laptop`), `page-promo-b` (`hardware`), `gb-flex-list`.
3. `vertical-sequencer` (10 videos: body, glowbar, G key, biometrics).
4. `page-promo-c` (`the-best-of-gemini-meets-our-most-advanced-laptops`), `gb-page-promo-2-up` for `magic-pointer` and `rambler`, then `carousel-list-cinematic` (6 videos).
5. `page-promo-b` (`better-together`), `gb-page-promo-2-up` for `cast-my-apps`, `continue-on-googlebook`, `quick-access`.
6. `page-promo-b` (`millions-of-apps`), `demo-tabpanels` (3 videos, 24 images), `page-promo-b` (`thousands-of-games`).
7. `page-promo-b` (`secure-by-design`), `card-list-a`, `extras-included`, a 2-up offers module, `shop-the-full-lineup`, a closing `page-promo-c` (`stay-in-the-loop`, newsletter).

### Images

- 40 `<img>` (33 `loading="lazy"`, 16 with `alt`), 37 `<picture>`, `decoding="auto"`. Each `<picture>` has `media` sources per breakpoint with explicit `width` and `height`, and `srcset` at 1x and 2x.
- URL grammar: `…/assets/images/<2 hex>/<2 hex>/<32 hex>/<file>.webp=n-w1410-h940-fcrop64=1,00000000ffffffff-rw`. `n-w<px>-h<px>` is the size, `fcrop64=1,<hex>` the focal crop, `-rw` requests WebP; icons use `=s144-fcrop64=1,…-rw` (long edge). 205 distinct image URLs in `src` and `srcset`, 124 of them `s<px>` square sizes (s48, s72, s96 and s144 for the 23 app icons, a few larger for other pictures), 45 distinct `n-w` widths between 267 and 4096.
- Responses: `Cache-Control: public, max-age=31536000`, `Access-Control-Allow-Origin: *`, `Accept-Ranges: bytes`, HTTP/3, no `Vary`, no `Timing-Allow-Origin` (cross-origin `transferSize` reads 0).
- Formats: a request with `-rw` returns `image/webp` (30,360 B for a 1410 px wide picture); the same URL without `-rw` and `Accept: image/avif,image/webp` returns `image/jpeg` (102,750 B). AVIF is never served. An `s144` icon is 930 B. The logo SVG is 33,263 B, `Vary: Accept-Encoding`, not compressed under `Accept-Encoding: br`.

### Video

- 30 `<video>` elements hold 421 `<source>`; 138 distinct files, 225,716,373 B if all were downloaded; 19 sequences. Every file is video only (no audio stream).
- Attributes: `muted autoplay playsinline preload="auto"`, `loop` except the hero, `aria-hidden="true"`, `tabindex="-1"`, no `poster` attribute (three poster images load at start through the `video` initiator).
- Source order per video: `(max-width: 599px)` 600w; `(min-width: 600px) and (max-width: 1023px)` 1024w; `(min-width: 1440px)` 1920w; then an unconditional 1440w. Inside each: AV1 MP4, VP9 WebM, H.264 MP4. The `type` attribute is only `video/mp4` or `video/webm`, so the AV1 and H.264 MP4 rows share one type (no `codecs=` parameter).
- File names `<name>-<W>w-<D>x-<codec>.<ext>`. `D` is the pixel density: `ffprobe` read `googlebook-hero-v2-frfr-1440w-2x-av1.mp4` as 2880x1620 AV1 Main yuv420p, 30 fps, 7.03 s, 1.61 Mb/s, and `body-1-1-600w-2x-vp9.webm` as 1200x1200 VP9 profile 0, 10.47 s, 455 kb/s.
- Ladder (files, median bytes): AV1 600w 19 / 359,265, 1024w 19 / 753,532, 1440w 4 / 1,414,395, 1920w 4 / 1,852,707; VP9 19 / 554,147, 19 / 811,793, 4 / 4,839,289, 4 / 7,886,728; H.264 19 / 233,507, 19 / 513,182, 4 / 1,822,566, 4 / 2,944,325. AV1 and VP9 ship as the 2x file (19 sequences each at 600w), H.264 mostly as the 1x file (13 of 19 at 600w, 6 as 2x). Only four sequences (hero, gradient, apps, games) have 1440w and 1920w rungs; the other 15 stop at 1024w. File names carry an aspect ratio for 54 files (1:1 24, 16:9 24, 16:10 6) and none for 84.
- Responses: `Cache-Control: public, max-age=3000` (50 minutes, with `Age`), `Last-Modified`, `Accept-Ranges: bytes` (206 on `Range`), no `ETag`, `Cross-Origin-Resource-Policy: cross-origin`.

### Loading, measured

| Viewport | Requests at load | After scrolling to the end | Video | Images |
|---|---|---|---|---|
| 1994 px, DPR 1.25 | 72 (1 video: hero 1920w 2x AV1; 8 images, 3 posters, 52 script entries) | 99 | 15 requests, 13 distinct files, 21,381,893 B, all AV1 (10 at 1024w, 4 at 1920w, 1 games file of 10,325,151 B = 48 % of the video bytes) | 20 WebP, 1,413,566 B |
| 375 px, DPR 2 | 66 (1 video: hero 600w 2x AV1, 912,098 B; 5 images, 44,124 B) | 102 | 13 distinct files, 11,552,982 B, all 600w AV1 | 20 WebP, 1,332,744 B |

- Videos below the hero request nothing until the section approaches the viewport: with 900 px scroll steps at 1994 px, the first video after the hero starts at scroll 2,700 px.
- Timing in the automation tab: load event 274 ms warm and 1,868 ms cold, first-contentful-paint 5,212 ms (a hidden tab delays paint, treat as an upper bound), no `largest-contentful-paint` entry reported (the first large element is a video).

### What we do differently (decisions, derived from the measurements)

- AVIF where the client accepts it, WebP next, JPEG or PNG last, through `Accept`, with `Vary: Accept` (googlebook serves WebP only).
- `<source type>` carries `codecs=` so AV1 and H.264 MP4 rows are distinct.
- One cache policy: content-hashed names are `immutable` for a year (googlebook videos revalidate every 50 minutes).
- `Timing-Allow-Origin: *` on the media host, so a page can measure its own bytes.
- The sprite is one cacheable file, not 84 KB repeated in every HTML.
- Pinned `.sha8` URLs stay valid after a republish.
- Brotli on text, and a text asset smaller than its gzip form (googlebook's 725 KB CSS gets no Brotli).
