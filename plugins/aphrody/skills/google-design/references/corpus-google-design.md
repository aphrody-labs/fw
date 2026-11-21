# design.google and Material 3 from the corpus

Distilled from the local corpus on `fr1` (`fw-corpus`, read 2026-11-21). Both sites are (c) Google: this file paraphrases and cites, it never reproduces pages. Re-query before relying on a spec: `fw-corpus search "<words>" --source m3|design-google`.

## Sources

| id | holds | fetch |
| --- | --- | --- |
| `design-google` | design.google library articles (server-rendered HTML); 27 library URLs answer HTTP 403 and are not stored (the two checked redirect to medium.com) | sitemap, HTML to markdown |
| `m3` | m3.material.io (Angular app) rendered in headless Chrome: components, styles, foundations, blog | sitemap, `google-chrome --dump-dom`; the page ships its whole token set as one 26 MB attribute, which the converter drops |

`m3` is the slowest source (about one page a minute on a loaded node): `fw-corpus status` shows how many of the sitemap URLs are stored, and `fw-corpus sync m3 --only /components/` fetches one section first.

## Buttons (M3 and M3 Expressive)

Source: `https://m3.material.io/components/buttons/specs` (fetched 2026-11-21).

- Two variants: default and toggle (selection); toggle exists only in M3 Expressive.
- Configurations: size small by default, with XS, M, L and XL added in Expressive; shape round by default, square added in Expressive; five colour styles (elevated, filled, tonal, outlined, text), and toggle buttons do not use the text style; the small button's 24dp padding is not recommended in Expressive, use 16dp.
- Anatomy: container, label text, optional icon. Colours are design tokens; other colour roles are acceptable as long as container and text keep a 3:1 contrast ratio.

## App bars

Source: `https://m3.material.io/components/app-bars/specs` (fetched 2026-11-21).

- Expressive variants: search, small, medium flexible, large flexible. The baseline medium and large bars are not recommended in Expressive; the flexible ones are visually close, support multi-line titles, are shorter and can hold images. Centre-aligned merges into small as a centred-text configuration.

## Research behind Expressive

- Google ran eye tracking, surveys and focus groups, experiments and usability studies on components (progress indicators, button size and tap time, the floating toolbar) and chose to exceed existing standards for tap target size and colour contrast in many cases; people of all ages preferred well-applied expressive design, up to 87% among 18 to 24 year olds. Source: `https://design.google/library/expressive-material-design-google-research#researching-material-3-expressive` and `#people-prefer-expressive-designs` (the page body states the figures; the skill `google-design` keeps the rules).
