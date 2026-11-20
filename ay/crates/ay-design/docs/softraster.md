# ay-design/softraster

Dependency-free, deterministic CPU triangle rasterizer: a z-buffer, perspective-correct nearest-texel
texturing and an alpha cutout, over an RGBA8 colour buffer and an `f32` depth buffer. It uses scalar `f32`
math only (no SIMD, no FMA), so the same input produces the same bytes on every target, which is what
golden-image tests need. It is `forbid(unsafe_code)` and has no dependencies, so it also builds for
`wasm32-unknown-unknown`.

Legacy published name: `aphrody-softraster` (crates.io, 1.4.7). This crate is the restored source under the
repository's own name; it is not published.

The crate works in screen space: project vertices with any camera model into `ScreenVertex` values
`(x_px, y_px, depth)` with `depth > 0` in front of the camera, then call `fill_triangle` per triangle.

```rust
use ay_design::softraster::fill_triangle;

let (w, h) = (8u32, 8u32);
let mut px = vec![0u8; (w * h * 4) as usize];
let mut z = vec![f32::INFINITY; (w * h) as usize];
fill_triangle(
    &mut px, &mut z, w, h,
    (0.0, 0.0, 1.0), (8.0, 0.0, 1.0), (0.0, 8.0, 1.0),
    [[0.0; 2]; 3], None, 1.0, [1.0, 0.0, 0.0],
);
```

## Test

```sh
cargo test -p ay-design/softraster
cargo clippy -p ay-design/softraster --all-targets -- -D warnings
```
