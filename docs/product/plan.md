# Plan: roadmap items as lots

Each roadmap item of `roadmap.md` is one lot, a file in `docs/agents/queue/` with Goal, Done, Remaining and Proof. A lot is closed only on its proof, never on an agent's own report: an independent run (CI or another session) rebuilds and checks the pushed SHA. At most three lots run at once.

| Id | Roadmap | Lot | Deliverables (paths) | Proof | Runs on | Depends on |
|---|---|---|---|---|---|---|
| P1 | M5 | `media-cdn` | `cloudflare/media-cdn`, `tools/media-cdn/{grammar,manifest,build,publish,verify,fixtures,sprite}.ts`, `layout.toml` entries | `verify.ts` green on the live host, unit tests, header and byte table in the lot | PC (Worker deploy), `fr1` (encode) | `edge-estate` for hostname and bucket |
| P2 | M5, M6 | `media-encode` | VMAF gate in `build.ts`, real sources under `$WORK/media-src`, `media.yml` workflow | manifest with VMAF per rung, bytes under G5 to G7 | `fr1`, `prd` | P1 |
| P3 | M6 | `fonts-delivery` | profile choice in `product.toml`, preload and fallback metrics in the page builder | per-page font bytes and CLS table | PC (Chrome) | `material-cdn` (live) |
| P4 | M6 | `reference-measure` | `tools/reference-measure.ts`, `docs/product/reference.json` | `--check` green, red on a changed value | PC (Chrome), cluster cron | none |
| P5 | M6 | `og-jsonld-pages` | `og` and JSON-LD helper in the page builder, head test | head test on every page | PC | none |
| P6 | M6, M7 | `web-gates` | `tools/web-gates.ts`, `docs/product/budgets.toml`, CI job | run over every page: bytes, LCP, CLS, axe, requests | PC (Chrome), CI | P3, P5 |
| P7 | M7 | `product-site-modules` | `tools/site/`, `gpui-kit` modules, pages for aphrody, fw, ay | screenshots at four widths, G4 to G7 numbers, axe clean | PC (Chrome), `fr1` (builds) | P1, P3, P5; `aphrody-gpui-m3-landing` |
| P8 | M7 | `scroll-scrub-hero` | web CSS stage, GPUI element, reduced-motion still | screenshots at five offsets, frame-time trace | PC (Chrome), `fr1` | P7 |
| P9 | M7 | `release-pages` | page generator from `release.toml` and the edge manifest, Atom feed, `[products.bun]` | page against the live manifest, links checked | PC | P7; `bun-fork-merge-optimise` |
| P10 | M8 | `product-pages-rest` | nie, shenron, Bun fork pages, shenron media decision | per-page screenshots and `web-gates` numbers | PC, shenron's own CI | P7, P9, `cloudflare-estate-and-shenron-cutover` |

## Order

1. P1 is done first and unblocks every media consumer.
2. P3, P4 and P5 are independent of each other and run in parallel (cap three).
3. P2 continues beside them on the cluster, since it is encoder time, not agent time.
4. P6 starts when P3 and P5 have landed, in report mode before the M6 branch point (2026-11-28).
5. P7 starts after the M6 branch; P8 and P9 follow it; P10 closes the train at M8.

## Rules for every lot

- Work on `main`; commit by path with a temporary index (`GIT_INDEX_FILE`, `git read-tree origin/main`, `git update-index --cacheinfo`, `git commit-tree -p origin/main`, `git push <sha>:refs/heads/main`), retry on rejection, never `git add -A`.
- Hostnames, buckets and Workers are created only by `edge-estate`'s estate file; a lot sends it the binding list and waits.
- New top-level paths are announced to the layout lot before they exist; new `.ts` files outside the pure-Rust baseline need a `[rustonly]` allow entry with a reason in `layout.toml`.
- Every number written into a doc comes from a command run in that session.
- Heavy encodes and builds run on `fr1` or `prd`, never on the PC.
