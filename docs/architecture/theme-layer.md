# Theme and settings layer (`crates/fw-theme`)

One declarative source for every color, icon, text and design token; no value is written twice. Modeled on Zed (settings JSON with a JSON Schema, live reload) and on omarchy (a theme is a directory, one switch recolors terminal, compositor, apps and wallpaper).

| Source | Owner | Read by |
| --- | --- | --- |
| `product.toml` `[brand]` seed and variant | brand | `fw-m3` (Material 3 roles), `fw-theme` (default theme) |
| `crates/fw-m3` | design tokens, color schemes, springs, shapes | every UI target |
| `themes/<name>/theme.toml` (+ omarchy `colors.toml`, `backgrounds/`, `templates/*.tpl`) | themes as files | `fw_theme::ThemeStore` |
| `ay/locales/<locale>/*.tsv`, `locales.toml` files (`[locales.<id>.messages]`) | text | `fw_theme::catalog::Catalog` |
| material.aphrody.com icon sets, `fw-material` | icons | `theme.toml` `[icons]`, web target |
| `settings.json` in `$FW_CONFIG_DIR`, `$XDG_CONFIG_HOME/fw` or `~/.config/fw` | the user's choices | `fw_theme::Settings` |

## Theme directory

`theme.toml` has `name`, `mode` (`dark`, `light`, `both`), `[m3] seed` (`#rrggbb` or `brand`) and `variant`, an optional explicit `[colors]` palette with the omarchy `colors.toml` names, `[terminal]` overrides, `[icons]` (Material Symbols `style`, `weight`, `fill`) and `[wallpaper]`. Without `[colors]` the theme is derived from the seed: Material 3 roles from `fw-m3`, ANSI and accent hues placed by HCT (`crates/fw-theme/data/derive.toml`). With `[colors]` the palette wins and the matching Material 3 roles are overridden, so CSS, GPUI and the terminal agree. An omarchy theme directory (`colors.toml`) is accepted as is. `crates/fw-theme/schema/theme.schema.json` is the schema.

## Targets

`fw theme render <name> --target css|gpui|rio|hyprland|web` prints one, `--out DIR` writes all. `fw theme switch <name>` stores the choice in `settings.json`, writes every target and `templates/*.tpl` of the theme to `$XDG_STATE_HOME/fw/theme/current/` (`theme.css`, `gpui-theme.json`, `rio-theme.toml`, `hyprland-colors.conf`, `theme.json`, `wallpaper.*`) and runs the `hooks` of the settings (by target or `all`, with `FW_THEME_NAME`, `FW_THEME_MODE`, `FW_THEME_DIR`). `fw theme watch` repeats this when settings, themes or templates change.

GPUI keys come from `crates/fw-theme/data/gpui-map.toml`, terminal and Hyprland files from `data/templates/*.tpl` (`{{ key | filter }}`, filters `hex`, `bare`, `rgb`, `rgb_css`, `hypr`, `hyprwa`, `u32`, `alpha hh`).

## Settings schema

`fw theme schema [settings|theme]` prints JSON Schema draft 2020-12; `fw theme schema --write` refreshes `crates/fw-theme/schema/*.schema.json` and `--check` fails when they are stale. Other components register `fw_theme::SchemaFragment { path, schema }` values merged by `fw_theme::schema::settings_schema(&fragments)`. Extension keywords for generated UIs: `x-icon`, `x-group`, `x-unit`, `x-restart-required`, `x-secret`.

## Enforcement

`bun tools/hardcoded.ts` measures colors, user-facing strings and icon names written as literals outside token, catalog and theme files (`tools/hardcoded.toml`); `--check` runs in CI and in `cargo xtask ci` against the per-file baseline `docs/research/hardcoded-baseline.tsv`. Numbers and the migration record: `docs/research/hardcoded-values.md`.
