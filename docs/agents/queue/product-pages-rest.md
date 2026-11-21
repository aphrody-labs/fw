# product-pages-rest: nie, shenron and Bun fork pages (queued)

Spec: section 2 and section 7 of the cahier; matrix row 27. Roadmap: M8. Depends on `product-site-modules`, `release-pages`; related `cloudflare-estate-and-shenron-cutover.md`, `nie-ievr-data-pipeline.md`, `wgpu-bun-wgpu.md`.

## Goal
Pages for nie (engine), shenron (dragonballfr.com) and the Bun fork on the shared modules, with the shenron media decision made and executed.

## Done
- shenron's site has 68 routes under `apps/site/src/app`; its `public/` already ships `hero-daima` as AVIF, WebP and PNG.
- nie has `apps/hello` and `apps/rg-mcp`, a `gui/` workspace and a `deploy/` folder; no public page.

## Remaining
1. Decide media origin for dragonballfr.com: `media.aphrody.com` (Cloudflare R2) or the Garage CDN of ADR 0009; write the decision in the shenron repository ADR list and in the cahier.
2. shenron: hero and banner videos on the ladder, OG and JSON-LD, budgets measured with `web-gates`.
3. nie: engine page from `nie --help`, `nie plugins`, `nie vfs stats`; no game asset shown.
4. Bun fork: one page per `bun:*` module from its docs, downloads after the `release.toml` row.

## Proof
Per page: Chrome screenshots at four widths, `web-gates` numbers under budget, links checked.
