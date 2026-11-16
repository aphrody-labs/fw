# Research datasets

Every dataset below was read live on 2026-11-16 and carries a per-row source URL; values not found in a source are written as `unknown`. Row counts are computed from the files, not estimated. The `replicate`, `os_replica` and recommendation fields are Aphrody planning decisions, not upstream facts.

## Index

| Dataset | Rows | What it holds |
|---|---|---|
| `google-apps.toml` | 250 `[[product]]`, 20 `[[company]]` | Google consumer, developer, Workspace and Cloud products and the Alphabet companies, with `replicate` (yes 76, absorb-upstream 109, later 31, no 34) |
| `google-sitemap.toml` | 46 `[[sitemap]]` | Sitemap indexes and child sitemaps of the Google properties, about 4.75 million URLs |
| `pixel-11.toml` | 78 `[[feature]]` | Pixel 11 series and Googlebook features by area, with `os_replica` (49 true, 29 false) |
| `cloudflare.toml` | 138 `[[product]]` | Cloudflare products with free-plan status, REST path counts and `replicate` (86 true, 52 false) |
| `servers-infra.toml` / `.md` | 14 `[[component]]`, 23 `[[benchmark]]`, 7 `[[db_benchmark]]` | hyper, axum, Apache, nginx, Caddy, Bun, Docker, systemd, PostgreSQL, Redis, SQLite: protocols, features, TechEmpower R23 and database figures |
| `ui-frameworks.toml` / `.md` | 19 `[[framework]]` | iced, egui, GTK, Qt, GPUI, libcosmic, Slint, Tauri, Rune, NSH, Flutter, Compose, React Native, Expo, Dioxus, Leptos, Svelte, Next.js, Bun fullstack |
| `chromium-stack.toml` / `.md` | 14 `[[component]]`, 3 `[[option]]` | Chromium, CEF, cef-rs, Chrome for Testing, V8, Skia, Dawn and the browser build options (ADR-3) |
| `stitch.toml` | 1 `[stitch]`, 13 `[[feature]]` | Google Stitch: launch, model, SDK, skills and DESIGN.md |

Related inputs: `../features.toml` (scope of every product, 242 rows after this pass), `../spec.md`, `../adr/0001` to `0006`, `../../brand/tokens/m3-tokens.json` and `m3-tokens.css` (Material 3 tokens), `../../brand/assets/` (icons, logo, manifest), `../../brand/character/`.

## Gaps recorded in the datasets

- `google-design-mirror`: the crawl of design.google and m3.material.io had not finished when this index was written, so there is no dataset for them yet; the Material 3 values come from `brand/tokens/`.
- `ui-frameworks`: no independent cross-framework native size, startup or memory benchmark; only vendor figures, labelled as such.
- `servers-infra`: TechEmpower has no stock Apache, standalone Caddy or Bun database entry; proxies are labelled.
- `cloudflare`: 26 products have free plan `unknown`; 42 have no REST path mapping.
- `google-apps`: platforms are `unknown` for 218 of 250 rows; sitemap gaps (Sheets, Slides, Calendar return 404) are in `google-sitemap.toml`.
- `pixel-11`: four rows cite press articles instead of Google pages and say so.

## Showcase

One Bun binary, `bun build --compile`, that serves everything above from embedded assets with no external service. Nothing is hardcoded: names come from `ay.toml`, tokens from `brand/tokens/m3-tokens.json`, rows from the TOML files at build time.

### Routes

| Route | Content | Source |
|---|---|---|
| `/` | Brand page: logo, wordmark, icon set, character | `brand/assets/`, `brand/character/` |
| `/design` | Design system: Material 3 colour roles (light and dark), type, shape, elevation, state, motion, and the components rendered with the CSS variables | `brand/tokens/m3-tokens.css` and `.json` |
| `/products` | One page per product in `ay.toml` (os, browser, search, cloud, craft, brand) with its mockup and its rows from `features.toml` and status counts | `docs/features.toml`, Figma mockups exported as images |
| `/datasets` | Gallery of every dataset in this folder: table view, filter, sort, source links, row counts | `docs/research/*.toml` |
| `/frameworks` | Desktop, web and mobile framework matrix and benchmarks, with the four-surface recommendation | `ui-frameworks.toml`, `servers-infra.toml` |
| `/cloud` | Cloudflare catalog and replicate plan, with free-plan filter | `cloudflare.toml` |
| `/google` | Google apps, Pixel 11 features and sitemap tree | `google-apps.toml`, `pixel-11.toml`, `google-sitemap.toml` |
| `/api/*` | JSON of every dataset and OpenAPI description; `/mcp` exposes the same as MCP tools | the same TOML |

### Rendering

- Pages are MDX files (`showcase/content/*.mdx`) compiled at build time; components such as `<Dataset file="cloudflare" />`, `<Tokens />`, `<ProductGallery />` and `<FeatureTable product="os" />` read the TOML and tokens.
- Styling uses only the Material 3 CSS variables from `brand/tokens/m3-tokens.css`; light and dark follow `prefers-color-scheme`.
- Server is `Bun.serve` with the routes above; data is embedded in the compiled binary, with `--dev` for hot reload from disk.
- Tests: `bun test` for the route table and dataset loaders; proof of each page is a real browser screenshot, not an HTTP status.

### Skills the showcase needs

| Skill | Purpose |
|---|---|
| `material-3` (exists) | Tokens, components and design review for `/design` |
| `showcase-dataset` | Add or refresh a dataset: crawl with sources and retrieval dates, write TOML with `unknown` for gaps, update this index |
| `showcase-page` | Write an MDX page and its components from a dataset or a product mockup |
| `figma-mockup` | Export a product mockup from Figma to `showcase/assets/` |
| `web-quality` (exists) | Lighthouse, Core Web Vitals and WCAG 2.2 AA gate for every page |
| `bun-native` (exists) | Bun-only APIs for the server, build and tests |

### Hooks the showcase needs

| Hook | Event | Purpose |
|---|---|---|
| dataset-parse | PostToolUse on `docs/research/*.toml` | Parse with `Bun.TOML` and fail on a missing `version`, `retrieved` or source |
| features-check | PostToolUse on `docs/features.toml` | Run `bun tools/features.ts --check` |
| readme-counts | Stop | Recompute the row counts in this index and fail on drift |
| tokens-sync | PostToolUse on `brand/tokens/` | Regenerate the `/design` page data and check contrast roles |
| showcase-build | PreToolUse before commit | `bun build --compile` and a smoke request on every route |

### Delivery order

1. Loader for the TOML datasets and tokens, with tests.
2. `Bun.serve` routes and the MDX pipeline.
3. `/design` and `/` from `brand/`.
4. `/datasets` and the per-dataset pages.
5. `/products` once the Figma mockups exist.
6. Compile the binary, screenshot every route, record the proofs in `docs/features.toml`.
