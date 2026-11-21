# web-gates: budgets, accessibility and privacy gates for pages (queued)

Spec: R6, R11 to R13, R15, G1 to G9, G13, G14; matrix rows 15 to 18. Roadmap: M6 report mode, M7 blocking. Related: `modern-web-apply-everywhere.md`.

## Goal
`tools/web-gates.ts` loads every generated page in Chrome at 375, 800, 1280 and 1920 px with a mobile profile, records requests, bytes (Resource Timing works because the media and font hosts send `Timing-Allow-Origin: *`), LCP, CLS and the axe-core result, and fails when a budget of `cahier-des-charges.md` section 5 is exceeded.

## Done
- Budgets written (section 5 of the cahier); the media host sends `Timing-Allow-Origin: *` (checked live).

## Remaining
1. `docs/product/budgets.toml` generated from the cahier table, with a `--check` that fails on drift.
2. The runner (Chrome through the `chromium` skill or `agent-browser`), results in the private store, not in git.
3. Checks: third-party request count 0, cookies 0, `forced-colors` and `prefers-reduced-motion` emulation screenshots, 48 px targets, contrast, `alt` coverage.
4. CI job in report mode, then blocking at M7.

## Proof
A run over `aphrody/site/public/*.html` with the measured numbers per page and breakpoint.
