# material.aphrody.com

> Status: the à-la-carte outputs and the extended static tree are written and unit-tested but not deployed or published yet; see `docs/agents/queue/material-cdn-a-la-carte.md`.

The Material CDN: Material Symbols, Google Sans Flex, Google Sans Code, Roboto Flex, M3 tokens, Aphrody icons and shared assets, served from R2 by one Worker. Two ways to use it:

1. **Static tree** under `/v1/`: everything prebuilt, content-addressed or versioned, immutable where the name allows.
2. **À la carte**: the Worker builds exactly the icons, characters and axes a request names (HarfBuzz hb-subset and a Rust WOFF2 encoder, both wasm) and caches the result in R2 and the Cache API.

`/v1/` answers with this index as JSON. Typed helpers build the URLs: `tools/material-cdn/client.ts` (Bun/TS) and `fw_material::cdn::Request` (Rust).

## Static tree

| Path | Content |
| --- | --- |
| `index.json`, `meta.json` | Catalogue (names, codepoints, tags, categories, sha256, sizes, sources) and the Worker's build metadata (font axes, cmap, revision) |
| `icons/<style>/[w<100..700>/]<name>[-fill].svg` | Every icon, three styles, seven weights, both fills (opsz 24, GRAD 0) |
| `sprites/<style>[-fill].svg` | One `<symbol id=name>` per icon, weight 400 |
| `fonts/material-symbols.css`, `fonts/material-symbols-<style>/variable.<sha8>.woff2` | The full variable fonts, all four axes (FILL 0..1, wght 100..700, GRAD -50..200, opsz 20..48) |
| `fonts/<family>.css`, `fonts/<family>/<subset>.<sha8>.woff2` | Full fonts and `unicode-range` subsets; profiles `google-sans-flex`, `-upright` (slnt 0), `-ui` (slnt 0, wdth 100), `google-sans-code` (+ italic), `roboto-flex`, `-upright` |
| `src/<family>.<sha8>.ttf` | The upstream variable TTFs the Worker subsets (OFL and Apache-2.0, immutable) |
| `data/symbols/<style>.json` | Icon names, codepoints and FILL glyph ids per style |
| `tokens/m3.css`, `m3.json`, `m3.dtcg.json` | `fw-m3/tokens/google_design_tokens.toml` as CSS custom properties (`--md-*`, colour roles per `[data-scheme]`), JSON and W3C DTCG |
| `aphrody/icons/<name>.svg`, `aphrody/index.json` | Our icons from `aphrody/assets/icons` verbatim (folders kept: `file_icons/rust`) |
| `assets/<name>/<version>/<file>` | Versioned assets of other workers (sprite sheets, GLB, textures), immutable; publish with `tools/material-cdn/asset.ts` |
| `rust/material_symbols.rs`, `rust/<version>.tar.gz` | Generated tables for `fw-material` |
| `licenses/` | Apache-2.0, OFL-1.1 texts and NOTICE |

Pinning: `?v=<sha256 prefix>` on any static file is immutable when it matches the object, a 404 when it does not.

## À la carte

Parameters (all optional unless stated; lists are comma separated):

| Parameter | Meaning |
| --- | --- |
| `style` | `outlined` (default), `rounded`, `sharp` |
| `set` | `material` (default) or `aphrody` (sprite, json, icon) |
| `icons` | Icon names (required for `icons.*`, `icon.svg`); at most 1000 |
| `text`, `unicodes`, `subsets` | Characters to keep: literal text, `20-7E,A0` (hex, `U+` allowed, a literal `+` in a query means a space), named ranges `latin`, `latin-ext`, `cyrillic`, `cyrillic-ext`, `greek`, `greek-ext`, `vietnamese`, `symbols` |
| `<axis>` | `fill`, `wght`, `grad`, `opsz`, `wdth`, `slnt`, `rond`, ... (any tag of the font, any case): `400` pins, `300..700` limits to a range |
| `axes` | `default` (axes not named are pinned to their default) or `all` (axes not named stay whole) |
| `features` | OpenType layout features to keep besides the defaults, `*` keeps all |
| `ids` | Sprite ids: `suffix` (`home`, `home-fill`) or `of` (`o-home`, `f-home`, what `aphrody/site/build.ts` inlines) |
| `family`, `class`, `display` | CSS outputs: `font-family`, class prefix (default `ms`), `font-display` |
| `size`, `color` | `icon.svg` |
| `v` | Data revision (`rev` in `meta.json`): makes the response immutable |

Outputs:

| URL | Result |
| --- | --- |
| `GET\|POST /v1/icons.svg` | SVG sprite of `<symbol>`s: `?style=rounded&icons=home,search&fill=0..1&wght=400&ids=of` |
| `GET\|POST /v1/icons.json` | `{ rev, style, wght, viewBox, icons: { home: { "0": "<d>", "1": "<d>" } } }` (Aphrody set: viewBox and body) |
| `GET /v1/icon.svg` | One icon: `?icons=home&style=sharp&fill=1&wght=500&size=24&color=%23fff` |
| `GET\|POST /v1/icons.woff2` | Ligature font with exactly these icons: `?icons=home,search&fill=0..1&wght=400..700&grad=0&opsz=24` |
| `GET\|POST /v1/icons.css` | `@font-face` (immutable `src`), `.ms` base class, `.ms-<icon>::before`, `.ms-fill` when FILL is variable |
| `GET /v1/fonts/<slug>.woff2` | Text subset: `?text=Aphrody&wght=400..700&wdth=100&features=ss01` |
| `GET /v1/fonts/<slug>.css` | With parameters: one `@font-face` per subset with `unicode-range` and immutable `src`; without: the static full CSS |
| `GET /v1/g/<hash>.<ext>` | Any generated output by its canonical hash (the `Content-Location` of every response; POST results are fetched this way) |

Static SVG outputs exist for opsz 24 and GRAD 0 only (weights in steps of 100); other axes need `icons.woff2`. A font request needs `text`, `unicodes` or `subsets`. `fonts/material-symbols-<style>.woff2|css` are aliases of `icons.woff2|css`.

How an icon subset stays exact: the Worker keeps the requested codepoints, the letters of their names and, when FILL can reach 1, the `.fill` glyphs (ids from `data/symbols`), and subsets with HarfBuzz's no-layout-closure flag, so only the ligatures of the requested names survive (`<span class="ms">home</span>` keeps working) instead of every icon spellable with those letters.

### Cache and headers

- Key: `sha256(algorithm, revision, output, canonical query)`, first 20 hex. Parameters are sorted, lists merged, text becomes `unicodes`, parameters that do not change the bytes (CSS names for a font) are dropped, so equal needs share one object.
- Lookup order: Cache API, R2 `v1/gen/<hash>.<ext>`, build. A built result is written to both.
- `ETag: "<hash>"`, `If-None-Match` answers 304. `Content-Location: /v1/g/<hash>.<ext>`.
- With `v=<rev>` (and in `g/`): `Cache-Control: public, max-age=31536000, immutable`. Without: `max-age=86400, s-maxage=86400, stale-while-revalidate=604800`. A `v` that is not the current revision is served only if that result was built before, else 404.
- `X-Material-Source: built | r2 | edge-cache` and `Server-Timing: gen;dur=` tell where a response came from.
- Text outputs (svg, json, css) are compressed to Brotli by the Cloudflare edge for clients that accept it; woff2 is already compressed.
- POST (`icons.svg|json|woff2|css`): a JSON object of parameters (`{"icons":[...],"fill":[0,1]}`) or a plain list of names, plus query parameters; for lists too long for a URL (limit 256 KB).

## Sources and lock

`tools/material-cdn/lock.json` pins google/material-design-icons and google/fonts by commit and every font blob by git SHA. `sync.ts` fetches exactly those commits; `sync.ts --update` moves the lock to the latest upstream commits (the daily workflow does this and republishes).

## Build, publish, deploy

```sh
bun tools/material-cdn/sync.ts [--update]          # sources at the pinned commits into .cache/material-cdn
bun tools/material-cdn/build.ts                    # /v1 tree into .cache/material-cdn-out (Linux: GNU tar for the crate tarball)
bun tools/material-cdn/extras.ts                   # tokens and Aphrody icons into the same tree
CLOUDFLARE_API_TOKEN=... bun tools/material-cdn/publish.ts    # only changed objects, index.json last
bun tools/material-cdn/asset.ts <name> <version> <file>...    # versioned asset, refuses to overwrite
cd cloudflare/material-cdn && bun install && bun run deploy    # builds woff2.wasm (Rust) and runs wrangler deploy
```

## Tests

```sh
cd cloudflare/material-cdn && bun test           # grammar, client fixtures, and the Worker against the built tree (skipped when .cache/material-cdn-out is absent)
cargo test -p fw-material cdn                    # the Rust helper against tools/material-cdn/fixtures/urls.json
uv run --with fonttools --with brotli tools/material-cdn/verify.py   # live: glyph and ligature counts of returned fonts, sprite symbols
```
