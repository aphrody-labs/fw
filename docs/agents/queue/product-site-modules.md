# product-site-modules: the shared module set for product pages (queued)

Spec: `cahier-des-charges.md` section 2, matrix rows 3, 7, 11, 13, 19, 20; R8 to R10, R16. Roadmap: M7. Depends on `media-cdn`, `fonts-delivery`; follows `aphrody-gpui-m3-landing.md` and `ui-stack-parity-matrix.md`.

## Goal
One generator builds the pages of aphrody, fw and ay from the measured module vocabulary, with data read from the repository, in the site-is-a-desktop layout (workspace, windows, dock), on M3 Expressive tokens.

## Done
- `aphrody/site/build.ts` generates `index.html`, `framework.html` and `studio.html` from workspace data (WGSL wallpaper, inlined Material Symbols, spring integrator from `fw-m3`).
- Measured module vocabulary: `google-design.md`, section "Page structure".

## Remaining
1. Extract the generator into a shared builder (`tools/site/`): input a `pages/<product>.toml` (module list, data sources), output HTML plus the budgets file.
2. Modules: hero stage, sequencer (N steps, one clip each), promo, 2-up, tab panels, carousel, apps grid, flex list; each as a window of the desktop, each with a keyboard path and focus ring.
3. `<picture>` and video `<source>` generation from `/v1/video/<name>.json` and `fw_material::cdn`, explicit `width` and `height`, `loading=lazy`, video `preload="none"` plus the IntersectionObserver start (R8), no autoplay under `prefers-reduced-motion` (R10).
4. Pages: aphrody overview and apps, fw overview, ay SDK reference.

## Proof
Chrome run per page at 375, 800, 1280 and 1920 px: screenshots inspected, requests at load against G4 to G7, `axe-core` clean.
