# ay-design/sprites

Turns a sprite atlas (one image plus named rectangles) into forms the web consumes:

- `SpriteSheet::to_css` / `to_css_mode`: one CSS class per region with a negative `background-position`
  (`CssMode::Image`), or `mask-image` with `currentColor` so a raster icon follows the text colour
  (`CssMode::Mask`).
- `SpriteSheet::to_svg`: a self-contained SVG with a single `data:` `<image>` and one `<symbol>` per region.
- `SpriteSheet::to_json`: the region manifest, for code.
- `sanitize_name` (region name to a valid CSS class / SVG id) and `data_uri` (base64 `data:` URI).

The module needs only `alloc` (no `std` item), has no dependency, and knows no input format: the caller supplies the atlas
image and the regions. Rectangles are copied, never recomputed.

Legacy published name: `aphrody-sprite-sheet` (crates.io, 1.4.7). This crate is the restored source under the
repository's own name; it is not published. The API is renamed to English (`to_css`, `CssMode`,
`sanitize_name`, `width`/`height`) and the class prefix is `sprite`.

## Test

```sh
cargo test -p ay-design --features sprites sprites
cargo clippy -p ay-design --features sprites --all-targets -- -D warnings
```
