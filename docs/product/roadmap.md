# Roadmap: product pages, media and delivery

Aligned to the release train of `release.toml` `[schedule]` (4-week cadence, branch point 3 weeks before stable, even milestones extended for 8 weeks, the year's last milestone is LTS for 104 weeks) and `docs/release/model.md`. Dates are the output of `bun tools/release-plan.ts plan --date <day>` read on 2026-11-21. Requirements (R) and goals (G) are those of `cahier-des-charges.md`; lots are in `docs/agents/queue/` and detailed in `plan.md`.

| Milestone | Branch from main | Stable | Support |
|---|---|---|---|
| M5 | 2026-10-31 | 2026-11-21 (now) | stable until M6 |
| M6 | 2026-11-28 | 2026-12-19 | extended until 2027-02-13; LTS until 2028-12-16 (last stable of 2026) |
| M7 | 2026-12-26 | 2027-01-16 | stable until M8 |
| M8 | 2027-01-23 | 2027-02-13 | extended until 2027-04-10 |

Beta of a milestone runs from its branch point to its stable date; on `release/<M>` only fixes land.

## M5 (stable today)

- Googlebook reference measured live and recorded (`google-design.md`).
- Specification, matrix, goals, this roadmap and the plan.
- Media host live: Worker `media-cdn`, bucket `aphrody-media`, `media.aphrody.com`; build, publish and verify tools (lot `media-cdn`, `media-encode` first half).

## M6 (branch 2026-11-28, stable 2026-12-19, LTS)

M6 is the LTS: what it ships must stay compatible for two years. Freeze before the branch: the URL grammar of `media.aphrody.com` (`/v1/img`, `/v1/video`, `/v1/sprite`, token names, rung file names) and manifest schema 1; additions are allowed, renames are not.

Before the branch point (7 days):
- `media-cdn` follow-ups: CI workflow that builds and publishes on the cluster, cache hit ratio read from analytics, `g` gravity token decision.
- `media-encode`: the ladder encodes real (non-synthetic) sources, with a quality gate (VMAF) recorded per rung.
- `fonts-delivery`: Google Sans Flex preload and `font-display` verified on the live pages.
- `reference-measure`: the tool and its weekly drift check.
- `web-gates`: budgets G4 to G9 and the accessibility checks in report mode, results written to the private store.
- `og-jsonld-pages`: generator wired into `aphrody/site/build.ts`.

During beta (2026-11-28 to 2026-12-19): budget tuning and fixes only, on `release/6`.

## M7 (branch 2026-12-26, stable 2027-01-16)

- `product-site-modules`: the shared module set (hero stage, sequencer, promo, 2-up, tab panels, carousel, apps grid) in `gpui-kit` and `site/build.ts`; aphrody, fw and ay pages built from it.
- `scroll-scrub-hero`: web (`animation-timeline: view()`) and GPUI implementations, reduced-motion path.
- `release-pages`: downloads, security and feed pages from `release.toml` and the edge manifest; the Bun fork gets its `[products.bun]` row.
- `web-gates` becomes blocking (CI fails on a budget or accessibility regression).

## M8 (branch 2027-01-23, stable 2027-02-13, extended until 2027-04-10)

- `product-pages-rest`: nie, shenron and Bun fork pages on the shared modules; the shenron media decision (media host or Garage) executed.
- `media-cdn`: focal crop and video-poster extraction on demand if the measured need exists.
- Review of every goal G1 to G16 against field data (75th percentile); targets revised from measurements, not from this document.

## Dependencies

- Every page lot depends on `media-cdn` (live) and `fonts-delivery`.
- `web-gates` needs `reference-measure` only for the weekly comparison; it runs on its own for our pages.
- Hostnames and buckets are created only through `docs/agents/queue/edge-estate.md`.
- Encodes and the long builds run on `fr1` and `prd`, never on the PC (`cluster/build.toml`).
