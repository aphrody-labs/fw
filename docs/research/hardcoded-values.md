# Hardcoded colors, user-facing strings and icon names (measurement)

Goal: no color, text or icon name is written as a literal outside the token, catalog and theme files; everything is generated from
`product.toml` [brand] through `fw-m3`, the locale catalogs, the material-cdn icon sets and the theme directories (`crates/fw-theme`).

Tool: `tools/hardcoded.ts`, rules and allowlist in `tools/hardcoded.toml` (the only place that declares which files may hold literals).
It reads blobs of a commit (`--rev`), removes comments, skips `#[cfg(test)]` modules, tests, fixtures, vendored and generated trees, and counts:

- color: hex literals, `rgb(`/`rgba(`/`hsl(`/`oklch(` with numeric arguments, named colors in color properties, Kotlin `Color(0x..)` and `Color.Red`;
- string: display text passed to UI constructors and builders (gpui `Label::new`, `.tooltip(`), TS/HTML text nodes and `title`/`placeholder`/`aria-label`/`alt`, Kotlin `Text(` and `contentDescription =`;
- icon: `icon`/`glyph` keys, `.svg`/`icons/` path literals, Material Symbols ligature elements, Kotlin `Icons.*`.

The heuristics are conservative (a literal needs letters and a space or a capital to be user-facing); the counts are a comparable measure, not an audit of every text.

## Commands

```
bun tools/hardcoded.ts --rev <commit> --by area --top 20     # counts of a commit
bun tools/hardcoded.ts --rev <commit> --by unit --top 30     # per crate/package
bun tools/hardcoded.ts --check                               # CI: fails when a file is above docs/research/hardcoded-baseline.tsv
bun tools/hardcoded.ts --update-baseline                     # after a reduction
```

CI runs `--check` in the `hardcoded` job of `.github/workflows/ci.yml` and as the `hardcoded` lane of `cargo xtask ci`. The baseline is per file, so existing debt does not fail
the build and any increase (or a new file with literals) does.

## Before (commit 38990f1dff0ee61577b655c1694c5903c3e1af14)

`bun tools/hardcoded.ts --rev 38990f1dff0ee61577b655c1694c5903c3e1af14 --by area --top 20`

```
rev 38990f1dff0ee61577b655c1694c5903c3e1af14: 5479 files scanned, 654 with literals
total 6693 (color 2031, string 4281, icon 381); inside allowlisted token/catalog/theme files: 2264 in 16 files
by area:
   4675  color   1696  string   2626  icon   353  aphrody
    653  color     45  string    598  icon    10  ay
    636  color     70  string    563  icon     3  packages
    345  color    216  string    121  icon     8  crates
    179  color      0  string    172  icon     7  apps
     87  color      0  string     87  icon     0  src
     82  color      0  string     82  icon     0  plugins
     16  color      0  string     16  icon     0  configs
      9  color      2  string      7  icon     0  tools
      6  color      2  string      4  icon     0  cloudflare
      5  color      0  string      5  icon     0  kotlin
```

`bun tools/hardcoded.ts --rev 38990f1dff0ee61577b655c1694c5903c3e1af14 --by unit --top 30`

```
by unit:
    835  color    312  string    500  icon    23  aphrody/tooling
    815  color    367  string    418  icon    30  aphrody/docs
    707  color    126  string    335  icon   246  aphrody/site
    533  color     48  string    482  icon     3  packages/ai-os
    219  color    161  string     58  icon     0  crates/fw-proto
    181  color     16  string    159  icon     6  aphrody/crates/ui
    178  color      0  string    171  icon     7  apps/chromium-dash
    168  color      0  string    168  icon     0  aphrody/crates/agent_ui
    163  color    150  string     13  icon     0  aphrody/templates
    127  color     60  string     52  icon    15  aphrody/.cloudflare
    124  color    107  string     17  icon     0  aphrody/crates/aphrody_craft
    121  color      0  string    121  icon     0  ay/packages/wck
    116  color      5  string    111  icon     0  ay/tools
    113  color    106  string      7  icon     0  aphrody/crates/nsh-platform-web
    100  color     22  string     78  icon     0  packages/gnu
     87  color      0  string     87  icon     0  src
     84  color      3  string     79  icon     2  aphrody/crates/aphrody_desktop
     75  color      0  string     75  icon     0  plugins/aphrody
     69  color      0  string     65  icon     4  ay/packages/org-mirror
     66  color      0  string     66  icon     0  aphrody/crates/settings_ui
     59  color     57  string      2  icon     0  aphrody/crates/nsh-platform-linux
     59  color      0  string     59  icon     0  ay/packages/automate
     59  color      7  string     52  icon     0  ay/packages/env-kit
     56  color      0  string     56  icon     0  ay/apps/auto-ui
     55  color      0  string     55  icon     0  aphrody/crates/language_models
     53  color      0  string     53  icon     0  aphrody/crates/git_ui
     50  color     49  string      1  icon     0  aphrody/crates/gpui-base
     50  color     50  string      0  icon     0  aphrody/crates/nsh-platform-gpui
     47  color      0  string     45  icon     2  aphrody/crates/collab_ui
     42  color     28  string     13  icon     1  ay/apps/auto-extension
```
