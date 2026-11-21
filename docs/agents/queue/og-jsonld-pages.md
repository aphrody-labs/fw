# og-jsonld-pages: Open Graph and JSON-LD generated from product.toml (queued)

Spec: R14, G15, matrix row 14; `docs/research/api-contracts-2026-11-21.md` item 4. Roadmap: M6.

## Goal
Every generated page carries `<html lang>`, a canonical link, the complete `og:*` set (`title`, `type`, `url`, `description`, `locale`, `site_name`, `image` with `:url :width :height :type :alt`), `twitter:card` and one JSON-LD object, all derived from `product.toml` and the page data.

## Done
- Reference measured: one JSON-LD `WebPage`, 15 `og:` and `twitter:` tags (`google-design.md`).
- `aphrody/site/build.ts` pages carry `<title>` and `lang`; no `og:` and no JSON-LD today (read from the source).

## Remaining
1. `og` helper in the shared page builder; `og:image` points to a `media.aphrody.com` rung of 1200x630 (AVIF is not accepted by every crawler: the `.jpg` form is used).
2. JSON-LD types per page (`SoftwareApplication` for downloads, `WebPage` otherwise).
3. Test: read the rendered head of every page and compare it with the generated data.

## Proof
The head test passes on every page; a link-preview render of one page checked in Chrome.
