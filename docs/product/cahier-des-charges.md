# Cahier des charges: product pages, media and delivery

Derived on 2026-11-21 from a live measurement of `googlebook.google/intl/fr_fr/` (build `googlebook-0.1.31-5b0f305`), recorded in `plugins/aphrody/skills/design/references/google-design.md` ("Googlebook product page, measured"). The page is a reference for structure, loading and delivery. Nothing of Google's text, images or video is copied; every number below is either measured there, measured on our own stack, or a published threshold that is named as such.

Companions: `roadmap.md` (milestones), `plan.md` (lots in `docs/agents/queue/`), `docs/release/model.md` (release train).

## 1. Rules that bind every page

- **Data, not prose.** A page shows files, folders, numbers and real values read from the repository at build time (`aphrody/site/build.ts` is the pattern). Each text item has a Material Symbol; actions run on the M3 springs of `crates/fw-m3/tokens/google_design_tokens.toml`.
- **Material 3 Expressive.** Colour roles come from the brand seed (`product.toml` `[brand]`, through `fw-m3`), type is Google Sans Flex and Google Sans Code (subsets from `material.aphrody.com`), shapes and motion from the same token file. No tinted lavender surfaces, pill-chip layouts, big-number heroes, uniform card grids or decorative particles.
- **The site is a desktop** (workspaces, windows, dock, freedesktop desktop entries); the googlebook modules below are layout vocabulary for those windows, not a marketing template.
- **Nothing fictional.** A module without a real feature behind it is not built. Where a product has no feature for a module, the matrix says so.
- **Pure Rust is the direction.** Web delivery code is Bun/TypeScript only where it is justified (Workers, build scripts), listed in `layout.toml` `[rustonly]`.

## 2. Products and surfaces

The module names are the measured googlebook module classes (hero stage, sequencer, promo, 2-up, tab panels, carousel, card list, flex list). Each surface names the real source of its data.

| Product | Surface | Modules and the real data behind them | Status |
|---|---|---|---|
| `fw` | Overview | Hero stage: a `fw` session capture on the video ladder. Sequencer, four steps with one clip each: install, `fw ask`, `fw serve` (chat plus `/mcp`, `/acp`, `/a2a`, `/v1/chat/completions`), `fw kernel`. 2-up: MCP/ACP/A2A tool registry, crate intelligence (`fw crates`). Source: `fw --help`, `README.md` Commands, `crates/fw-*` | missing |
| `fw` | Downloads | Table from `release.toml` `[products.fw]`: Linux musl binary, Windows exe, `fw_wasm.wasm`; sizes and SHA-256 from the release manifest served by `ay/apps/edge/src/manifest.ts` | partial (manifest exists, page does not) |
| `fw` | Security | SBOM (`fw export`, CycloneDX 1.5), `deny.toml` bans, `SHA256SUMS` of every release, as real counts read from the files | missing |
| `aphrody` | Overview | Hero stage: the desktop (WGSL wallpaper plus a clip, as `aphrody/site/public/index.html`). Sequencer: compositor, session (`crates/aphrody_session`), installer (`crates/aphrody_installer`), Studio (`crates/zed`, binary `aphrody`). | partial (`aphrody/site/build.ts` generates `index.html`, `framework.html`, `studio.html`) |
| `aphrody` | Apps and games | Grid of desktop entries with icons at s48, s72, s96, s144 (the googlebook ladder), from `aphrody/assets/icons` and the desktop entries | missing |
| `aphrody` | Downloads | `release.toml` `[products.aphrody]`: `aphrody-x86_64.tar.gz`, `aphrody-aarch64.tar.gz`, `Aphrody-Setup-x64.exe` | missing |
| `ay` | SDK reference | C ABI (`ay/abi`), corelib, OpenAPI (`ay/docs/auto/openapi.json`), design tokens (`ay/brand`). Tab panels: one tab per language binding | missing |
| `ay` | Downloads | `release.toml` `[products.ay]`: CLI for Linux x86_64 and aarch64 and Windows, four corelib wasm builds | missing |
| `nie` | Engine | Hero stage: the VFS browser. Sequencer: `nie vfs` index, decode, wgpu render, the plugin host. No game asset is shown or shipped (the installation of the player is the only input) | missing |
| `shenron` | dragonballfr.com | Existing site: 68 routes under `apps/site/src/app` (wiki, news, games, rankings, shop, download, stats, studio, tv ...). The googlebook measurements apply to its hero, banners and videos | partial |
| Bun fork | Runtime | Page per `bun:*` module (`bun:wgpu` in `docs/agents/queue/wgpu-bun-wgpu.md`, `bun:linux`), downloads. `release.toml` has no `[products.bun]` row | missing |
| all | Media host | `media.aphrody.com`: AVIF/WebP/fallback images with `=w`/`=s`, AV1/VP9/H.264 ladder, sprite, hashed immutable URLs | live (section 6) |
| all | Fonts and icons | `material.aphrody.com`: Google Sans Flex subsets, Material Symbols, M3 tokens | live |

## 3. Requirements

Identifiers are used by the matrix, the lots and the gates.

**Delivery**
- R1. Every image is served as AVIF when the client accepts it, else WebP, else JPEG (opaque) or PNG (alpha), from one URL, with `Vary: Accept`.
- R2. Image resizing by URL: `=w<px>`, `=h<px>`, `=s<px>` (long edge), `=q<30..95>`; ladders prebuilt at build time, other sizes on demand and stored.
- R3. Video is a ladder of AV1, VP9 and H.264 at 600, 1024, 1440 and 1920 CSS px, at 1x and 2x pixel density, never upscaled, produced by our own pipeline (ffmpeg on the cluster nodes, never the PC).
- R4. Every `<source>` of a video carries `type` with `codecs=`; the order is AV1 2x, VP9 2x, H.264 1x for each breakpoint (600, 1024, 1440).
- R5. Hashed names (`<name>.<sha8>`) are `public, max-age=31536000, immutable`; names that follow the manifest revalidate through the ETag; a pinned URL stays valid after a republish.
- R6. One cacheable SVG sprite per set; Brotli on every text response; HTTP/3 on every host.
- R7. Media responses send `Access-Control-Allow-Origin: *`, `Timing-Allow-Origin: *`, `Accept-Ranges: bytes`.

**Loading**
- R8. Only the hero media loads before the first scroll; media below the fold request nothing until the section is within one viewport height (measured on googlebook: the first video after the hero starts at 2,700 px of scroll).
- R9. Every image has explicit `width` and `height` (no layout shift) and `loading="lazy"` below the fold.
- R10. A video has no audio track, `muted`, `playsinline`, `aria-hidden="true"` and a poster; under `prefers-reduced-motion: reduce` it does not autoplay.

**Accessibility**
- R11. WCAG 2.2 AA: text contrast 4.5:1 (large text 3:1), targets 48 px, visible focus, keyboard path for every action.
- R12. `forced-colors: active` and `prefers-reduced-motion` handled for every component (googlebook: 137 and 24 CSS blocks).
- R13. 100 % of non-decorative images have `alt` (googlebook: 16 of 40).

**Structure and privacy**
- R14. Every page has `<html lang>`, a canonical link, complete `og:*` tags (`title`, `type`, `url`, `description`, `locale`, `site_name`, `image` with `:url :width :height :type :alt`), `twitter:card` and one JSON-LD object, generated from `product.toml` (see `docs/research/api-contracts-2026-11-21.md`, item 4).
- R15. No third-party request at load and no non-essential cookie, hence no consent bar (googlebook loads a 79,721 B consent script, a 9,139 B consent stylesheet and Tag Manager).
- R16. Layout steps follow the Material 3 window size classes; media rungs follow the measured 600/1024/1440 ladder.

## 4. Matrix: googlebook pattern, our equivalent, status, owner lot

Status on 2026-11-21: `live` (deployed and verified), `built` (in the repository, not deployed or not consumed), `partial`, `missing`, `decision` (we deliberately differ).

| # | googlebook pattern (measured) | Our equivalent | Req | Status | Lot |
|---|---|---|---|---|---|
| 1 | Images as `.webp=n-w<px>-h<px>-fcrop64=1,<box>-rw`, `=s<px>` icons, WebP only | `/v1/img/<name>[.<sha8>][.<fmt>]=w<px>-h<px>\|s<px>-q<n>`, AVIF, WebP, fallback | R1 R2 | live | `media-cdn` |
| 2 | Focal crop `fcrop64` | `g` gravity token mapped to the Images binding `gravity` | R2 | missing | `media-cdn` |
| 3 | `<picture>` with `media` per breakpoint, 1x/2x `srcset` | `fw_material::cdn` and `tools/media-cdn` URL helpers plus a `<picture>` generator in `site/build.ts` | R2 R9 | missing | `product-site-modules` |
| 4 | 138 video files, AV1/VP9/H.264, 600/1024/1440/1920, 1x/2x | `tools/media-cdn/build.ts`: the same matrix, 24 rungs for a 3840 px source | R3 | live | `media-encode` |
| 5 | `<source>` rows by breakpoint, `type` without codecs | `/v1/video/<name>.json` with `codecs=` types | R4 | live | `media-cdn` |
| 6 | Video `Cache-Control: max-age=3000`, no ETag | immutable hashed names, ETag on the rest | R5 | live | `media-cdn` |
| 7 | Lazy video start near the viewport | `loading=lazy` for images; a 40-line IntersectionObserver snippet for video, `preload="none"` | R8 | missing | `product-site-modules` |
| 8 | 90-symbol sprite inlined in the HTML (84,030 characters) | `/v1/sprite/<name>.svg` plus `material.aphrody.com` à-la-carte `icons.svg?icons=`, used symbols inlined per page by `site/build.ts` | R6 | partial | `media-cdn` |
| 9 | Google Sans static families through Google Fonts (151,980 B of CSS) | Google Sans Flex subset from `material.aphrody.com` (latin, wght 300..700: 41,836 B woff2, measured) | R16 | live | `fonts-delivery` |
| 10 | `--md-sys-*`, `--md-ref-*` tokens | `material.aphrody.com/v1/tokens/m3.css` from `fw-m3` | n/a | live | none |
| 11 | Breakpoints 600, 1024, 1440 (and 1620) | M3 window size classes for layout, 600/1024/1440 for media rungs | R16 | partial | `product-site-modules` |
| 12 | Scroll-scrub video hero (2,136 px section, pinned 1,068 px stage) | CSS `animation-timeline: view()` on the web, a GPUI scroll-driven element natively | R12 | missing | `scroll-scrub-hero` |
| 13 | Modules: card-list, promo-b, promo-c, 2-up, vertical sequencer, cinematic carousel, tab panels, flex list | One component per module in `gpui-kit` and in `site/build.ts`, data from the repository | R1..R16 | partial | `product-site-modules` |
| 14 | JSON-LD `WebPage` and 15 `og:`/`twitter:` tags | Generated from `product.toml`; the current `site/build.ts` pages carry `<title>` and `lang` only (no `og:`, no JSON-LD) | R14 | missing | `og-jsonld-pages` |
| 15 | Cookie consent bar and Tag Manager | No third-party request, no consent bar | R15 | decision | `web-gates` |
| 16 | CSS 725,075 B raw (85,187 B gzip, no Brotli), JS 44,054 B | Budgets in section 5, Brotli | R6 | missing | `web-gates` |
| 17 | `forced-colors` 137 blocks, `prefers-reduced-motion` 24 blocks | Same coverage, checked by a gate; `site/build.ts` has one `prefers-reduced-motion` rule today | R12 | partial | `web-gates` |
| 18 | `alt` on 16 of 40 images | `alt` on every non-decorative image, checked | R13 | missing | `web-gates` |
| 19 | Videos `aria-hidden`, `tabindex=-1`, `muted`, no audio | Same, plus reduced-motion no autoplay | R10 | missing | `product-site-modules` |
| 20 | Apps and games grid, 23 app icons at s48/s72/s96/s144 | Desktop-entry grid; icons as `=s` rungs from the media host | R2 | missing | `product-site-modules` |
| 21 | "Hardware" and OEM logos | Supported targets from `release.toml` (os, arch, abi per asset); no logos of third parties | n/a | missing | `release-pages` |
| 22 | Security module | Real signals: SBOM (`fw export`), `deny.toml`, `SHA256SUMS` of each release | n/a | missing | `release-pages` |
| 23 | Promo shop and "Extra perks" | Downloads from `release.toml` through the edge manifest; no perks module (nothing real behind it) | n/a | decision | `release-pages` |
| 24 | Newsletter form | Atom feed of releases generated from `release.toml`; no address collected | R15 | decision | `release-pages` |
| 25 | HTTP/3 (`Alt-Svc: h3`) | Cloudflare edge, header checked live | R6 | live | `media-cdn` |
| 26 | Reference measured by hand | `tools/reference-measure.ts` re-measures and fails on drift | n/a | missing | `reference-measure` |
| 27 | Product pages for nie, shenron, Bun fork | Section 2 | n/a | partial | `product-pages-rest` |

## 5. Goals with measurable targets

Targets are ours; the "basis" column says what they are derived from. `lab` means Chrome with a throttled mobile profile run by `tools/web-gates.ts` (lot `web-gates`); `field` means real users, measured at the 75th percentile.

| Id | Metric | Target | Basis |
|---|---|---|---|
| G1 | Largest Contentful Paint | field p75 <= 2,500 ms; lab <= 2,000 ms | web.dev Core Web Vitals "Good" threshold; googlebook's own first contentful paint was 5,212 ms in the hidden automation tab and it reported no LCP entry |
| G2 | Interaction to Next Paint | p75 <= 200 ms | web.dev "Good" |
| G3 | Cumulative Layout Shift | <= 0.1 | web.dev "Good"; R9 |
| G4 | First load, <= 599 px, DPR 2 (bytes transferred) | <= 700,000 B: HTML 40,000, CSS 30,000, JS 40,000, font 42,000, hero video 450,000, images 98,000 | googlebook loads 912,098 B of hero video alone at this size; font from the live subset (41,836 B) |
| G5 | First load, 600 to 1023 px | <= 1,200,000 B; hero rung AV1 1024w 2x <= 900,000 B | googlebook median AV1 1024w 753,532 B |
| G6 | First load, 1024 to 1439 px | <= 1,900,000 B; hero rung AV1 1440w 2x <= 1,600,000 B | googlebook median AV1 1440w 1,414,395 B |
| G7 | First load, >= 1440 px | <= 2,500,000 B; hero rung AV1 1920w 2x <= 2,100,000 B | googlebook median AV1 1920w 1,852,707 B, hero 1,852,707 B |
| G8 | Whole page scrolled to the end, <= 599 px | <= 8,000,000 B | googlebook 11,552,982 B video plus 1,332,744 B images |
| G9 | Whole page scrolled to the end, >= 1440 px | <= 15,000,000 B and no single file above 5,000,000 B | googlebook 21,381,893 B video, one file of 10,325,151 B |
| G10 | Codec ladder | 100 % of videos have the AV1, VP9 and H.264 rungs that fit their source; AV1 listed first; every `type` has `codecs=` | R3 R4, build manifest |
| G11 | Image format share | >= 95 % of image responses to a Chromium client are AVIF; 100 % have a WebP and a fallback | R1 |
| G12 | Cache | 100 % of hashed media responses `immutable`; media cache hit ratio >= 95 % at the edge after warm-up | R5, Cloudflare analytics |
| G13 | Request count at load | <= 40 requests, 0 third-party | googlebook 72 at 1994 px, 66 at 375 px; R15 |
| G14 | Accessibility | axe-core: 0 serious or critical violations on every page; contrast >= 4.5:1; 48 px targets; every action keyboard-reachable; `forced-colors` and `prefers-reduced-motion` screenshots reviewed | R11..R13 |
| G15 | Structured data | 100 % of pages pass an `og:*` and JSON-LD check generated from `product.toml` | R14 |
| G16 | Reference freshness | `tools/reference-measure.ts --check` passes weekly against the live reference | matrix row 26 |

## 6. State of the media host on 2026-11-21

`media.aphrody.com` is the Worker `media-cdn` (`cloudflare/media-cdn`) on the R2 bucket `aphrody-media`, with the grammar in `tools/media-cdn/grammar.ts` (R1 to R7 are implemented). Live check on 2026-11-21: `tools/media-cdn/verify.ts` passed 987 checks and 0 failed against the host (bytes equal to the manifest, magic numbers, headers, negotiation, ranges, on-demand sizes); the evidence and the rung sizes are in `docs/agents/queue/media-cdn.md`. Sizes of rungs measured so far come from synthetic fixtures; the byte budgets G4 to G9 are checked against real sources in `media-encode`.

## 7. Open points (queued, work continues)

- Shenron plans a self-hosted Garage CDN (`docs/decisions/0009-auto-hebergement-rust-integral.md` in the shenron repository): whether dragonballfr.com media come from `media.aphrody.com` or from Garage is decided in `product-pages-rest`.
- `site/build.ts` pages are generated for aphrody only; the generator becomes shared (one builder for every product page) in `product-site-modules`.
- The Bun fork has no row in `release.toml`; adding it is part of `release-pages`.
