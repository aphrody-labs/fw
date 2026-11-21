# fonts-delivery: Google Sans Flex on every page (queued)

Spec: R16, matrix row 9, G4. Roadmap: M6.

## Goal
Every product page loads Google Sans Flex (and Google Sans Code where code is shown) from `material.aphrody.com` as unicode-range subsets, preloaded, with `font-display` set and no layout shift.

## Done
- Live on `material.aphrody.com`: `/v1/fonts/google-sans-flex.woff2?subsets=latin&wght=300..700&axes=default` is 41,836 B, `wght=400` 18,964 B, all axes 1,048,100 B (measured 2026-11-21 with `curl`).
- CSS profiles `google-sans-flex`, `-upright`, `-ui`, `google-sans-code`, `roboto-flex` (`cloudflare/material-cdn/README.md`).

## Remaining
1. Pick the profile per surface (`-ui` for dense UI, full Flex for display text) and put the choice in `product.toml`, read by `site/build.ts`.
2. `<link rel="preload" as="font" crossorigin>` for the one subset used above the fold; fallback metrics (`size-adjust`, `ascent-override`) computed from the font files so the swap does not shift.
3. Animated axes (`wght`, `wdth`, `ROND`) in the sorted axis order that Motion interpolates.
4. Measure per page: font bytes, swap shift (CLS), first text paint.

## Proof
Per-page table of font bytes against G4 (42,000 B) and CLS 0 from the swap, in a Chrome run with screenshots.
