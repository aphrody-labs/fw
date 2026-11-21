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
| `anomalyco.toml` / `.md` | 86 `[[repo]]`, 226 `[[package]]`, 4 `[[site]]`, 5 `[[api]]` | The anomalyco (ex sst) org replica summary: opencode and 12 repos replicated in full (37,274 issues, 46,315 pull requests, 66,745 commits, 1,100 releases) against GitHub's totals, languages, ecosystem tags, npm and crates.io packages, opencode.ai and sst.dev mirrors, opencode's OpenAPI spec |

Related inputs: `../features.toml` (scope of every product), `../spec.md`, `../adr/`, `../../brand/tokens/m3-tokens.json` and `m3-tokens.css` (Material 3 tokens), `../../brand/assets/` (icons, logo, manifest), `../../brand/character/`.

## Gaps recorded in the datasets

- `google-design-mirror`: the crawl of design.google and m3.material.io had not finished when this index was written, so there is no dataset for them yet; the Material 3 values come from `brand/tokens/`.
- `ui-frameworks`: no independent cross-framework native size, startup or memory benchmark; only vendor figures, labelled as such.
- `servers-infra`: TechEmpower has no stock Apache, standalone Caddy or Bun database entry; proxies are labelled.
- `cloudflare`: 26 products have free plan `unknown`; 42 have no REST path mapping.
- `google-apps`: platforms are `unknown` for 218 of 250 rows; sitemap gaps (Sheets, Slides, Calendar return 404) are in `google-sitemap.toml`.
- `pixel-11`: four rows cite press articles instead of Google pages and say so.
