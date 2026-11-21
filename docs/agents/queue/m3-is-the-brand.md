# Lot: Material 3 is the brand. Purge every custom M3 and auto-sync the real one

Order of the owner, 2026-11-21: "notre marque c'est M3, pas Aphrody, et nos vieux M3 custom". Gather all the slop, purge it, and replace it with the real thing, auto-synced end to end.

## Purge (find every one with `git grep` on fw main; the list below was seen this session)

- `product.toml` [brand] `seed = "#e2dbad"` / `tonal-spot`, and `aphrody/product.toml` `seed_hex = "#135B82"` / `expressive` with its `[colors.channels.*]` RGB tables.
- `crates/fw-m3`, the custom scheme generation and `tokens/google_design_tokens.toml`, plus every CSS `--md-sys-*` value generated from a custom seed (for example `crates/fw-proto/ui/app.css`).
- `aphrody/crates/brand`, `aphrody/assets/os/themes.json`, ay `brand/tokens/m3-tokens.json`, the Kotlin `aphrody-theme` (`kotlin/`), and every hardcoded palette in the GPUI, NSH, web and Compose code.
- Docs and skills that describe the custom brand: the `google-design`, `design`, `aphrody-brand` and `kotlin-compose-m3` skills, the global CLAUDE.md design line (already updated), and DESIGN.md files.

## Replace with the real M3, auto-synced

- **Single upstream source:**
  - androidx Compose material3 tokens `compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/*.kt`: ColorLight/Dark, Palette, TypeScale, Shape, ExpressiveMotion, StandardMotion, Elevation, State;
  - material-components/material-web tokens for CSS names;
  - material-color-utilities only for the dynamic colour M3 itself defines.
- **One fw command, `fw m3 sync`** (Rust), on a schedule (hourly, like forks-follow). It reads those files at a pinned androidx commit, records the pin, and generates:
  - CSS custom properties `--md-sys-color-*`, `--md-sys-typescale-*`, `--md-sys-shape-*`, `--md-sys-motion-*`;
  - Rust constants for GPUI and wgpu;
  - Kotlin, where Compose `MaterialTheme` is used directly with no wrapper theme;
  - JSON for the Figma variables of the M3 Design Kit (Figma file vEr6T5SQaJfMpLxjMUfD55) and for Stitch DESIGN.md;
  - `--check` mode in CI. A PR opens when upstream changes.
- **Android and Compose:** use `MaterialTheme` with the default `lightColorScheme()`/`darkColorScheme()` and dynamic colour as in Now in Android's `NiaTheme`; no custom theme layer.
- **Guard:** a CI check that fails on any colour, radius, font or motion literal outside the generated files, and on any reference to the purged seeds.

## Proof

- **Pin:** the upstream pin is recorded.
- **Equality:** a test asserts that every generated value equals the androidx token, at least the colour roles, type scale, shapes and springs.
- **Rendered check:** surfaces are rendered in light and dark and inspected by screenshot.
- **Counts:** the number of literals before and after, with the commands that produced them.
