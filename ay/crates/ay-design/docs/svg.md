# ay-design/svg

The SVG toolkit of the SDK. Anything that parses, cleans, recolours, merges, optimizes, rasterizes or
compares SVG uses this crate instead of carrying its own implementation. Everything works on `&str` in
memory; no module touches the filesystem, so it is safe for FFI, WebAssembly and servers.

| Module | Feature | What |
| --- | --- | --- |
| `sanitize` | always | Element allowlist (passive SVG only; unknown and foreign elements go with their subtree); animation of `href`/`src`/`on*`/`style` dropped and animation values checked for script URLs; `on*` and `xml:base` removed; URL attributes must be same-document or raster `data:` images; CSS (`style`, `<style>`, any value with `(` or `\`) is normalized (comments, escapes) and refused on `expression(`, script URLs, `image-set(`, `@import`/`@font-face`/`@namespace` or an unsafe `url()`; comments, DTDs (rejected); optional external refs and editor metadata. |
| `viewbox` | always | Parse `viewBox`, absolute lengths to px, intrinsic size, synthesize a missing `viewBox`, `Keep`/`Responsive`/`Fixed` sizing. |
| `theme` | always | Bake `currentColor`, `var(--x, fallback)` and colour remaps in attributes, `style` and `<style>`; `Theme::m3` takes `(role, colour)` pairs and exposes `--md-sys-color-<role>`. |
| `sprite` | always | `<symbol>` sheets from standalone SVGs; internal ids and `url(#...)`/`href="#..."` prefixed per symbol. |
| `glyph` | `glyph` | `skrifa` outline pen (`SvgPathPen`) and glyph to SVG path/metrics, variable axes, collections. |
| `symbols` | `symbols` | Material Symbols by name: GSUB ligature lookup, 960-grid paths, gstatic layout, WOFF2 decoding; `symbol_svg` escapes its `fill` and `path` arguments. |
| `optimize` | `raster` | usvg normalizing writer with bounded precision (the Rust counterpart of svgo `preset-default`). |
| `raster` | `raster` | resvg to straight RGBA / PNG, ICO packing (PNG entries), favicon and Windows size sets. Embedded `data:` images render; path and URL `href`s are never read (no local file or network access); output is capped at `MAX_PIXELS` (64 million). |
| `diff` | `raster` | Pixel diff of two SVGs or RGBA buffers (max/mean delta, mismatch %). |

Default features: `raster`, `glyph`. Backends are pure Rust: resvg/usvg/tiny-skia, roxmltree, svgtypes, skrifa.

```rust
use ay_design::svg::{raster, sanitize, theme::Theme};

let clean = sanitize::sanitize(untrusted, Default::default())?;
let themed = ay_design::svg::theme::apply(&clean, &Theme::new().current_color("#6750a4"))?;
let ico = raster::render_ico(&themed, raster::WINDOWS_ICO_SIZES)?;
```

Legacy published name: `aphrody-svg` (crates.io, 1.4.7). This crate is the restored source under the
repository's own name; it is not published. Differences from the legacy crate: the `m3` feature (a path
dependency on a Material token crate) is gone, `Theme::m3` is always available and takes plain
`(role, colour)` pairs; the glyph integration test carries its own font fixture,
`tests/fixtures/GoogleSansCode-Variable.ttf` (SIL OFL 1.1, licence next to it).

## Test

```sh
cargo test -p ay-design/svg --all-features
cargo clippy -p ay-design/svg --all-targets --all-features -- -D warnings
cargo clippy -p ay-design/svg --no-default-features -- -D warnings
```
