# Reusable Cloudflare R2 Media & 3D Assets in Pure Rust (`wgpu` v30 + `wasm32`)

Machine-readable manifest: [assets/r2-catalog.toml](../assets/r2-catalog.toml), enforced in Rust by `ay-cf-assets` (`spdx` + `blake3` + `gltf` + `ktx2` + `etagere` + `thumbhash` + `http-range-header`) and in TypeScript by `@ay-cf/control-plane` + `ay-cf-gateway`.

## Zero-Egress R2 Layout (`assets.aphrody.app` & `video.aphrody.app`)

- **Content-Addressed Keys**: `v1/<kind>/<blake3>.<ext>` (`icon`, `ui`, `hdri`, `loop`, `lottie`, `tex`, `glb`, `index`), served via R2 Custom Domains + Zone Cache Rules (`public, max-age=31536000, immutable`, `Origin Range Requests: On`, `512 MiB` edge cache object ceiling).
- **Seekable Binary Streams**: Leave `Content-Encoding` unset (`identity`) on `.glb`, `.ktx2`, `.mp4`, and zstd-seekable packs so Cloudflare HTTP `Range: bytes=...` (`206`) byte offsets remain exact. Encode AV1 video loops with `-movflags +faststart` and fixed 2s GOP keyframes (`-g 60 -keyint_min 60 -sc_threshold 0`).

## Audited Pure-Rust Crate Stack

| Domain | Verified Crates | Critical Integration Notes |
| :--- | :--- | :--- |
| Icons & Vector SVG | `usvg` `0.50`, `resvg` `0.50`, `vello` `0.12`, `vello_gpu` `0.4`, `vello_svg` `0.11`, `kurbo` `0.13`, `lyon_tessellation` `1.0`, `lucide-icons` `1.74`, `icondata` `0.7` | `vello 0.12` & `vello_gpu 0.4` target `wgpu 30`; use `vello_svg` `default-features = false` + `RenderSink` bridge; disable `lucide-icons` default `iced` feature. |
| UI Atlas & R2 Storage | `etagere` `0.3`, `guillotiere` `0.7`, `blake3` `1.8` (`pure`), `ruzstd` `0.9.1`, `rkyv` `0.8`, `postcard` `1.1`, `opendal` `0.59` | Enable `blake3` `pure` feature for zero `cc`/asm; use `ruzstd >= 0.9.1`; pair `reqwest 0.13` + `reqsign-aws-v4` on `wasm32` (since `object_store` S3 excludes `wasm32`). |
| Images & HDRI | `image` `0.25.10`, `image-webp` `0.2`, `png` `0.18`, `qoi` `0.4`, `exr` `1.74.3`, `fast_image_resize` `6.1`, `thumbhash` `0.1` | Disable `image` default `avif` (avoids C `dav1d`); disable `exr` default `rayon` on `wasm32`. |
| Video | `mp4-atom` `0.17`, `matroska-demuxer` `0.8`, `rust_h264` `0.6`, `rusty_h264` `0.17`, `rust_h265` `0.4`, `gif` `0.14` | Avoid C FFI wrappers (`openh264`, `webm`); gate `rav1d` to native only (uses `libc`/`std::thread`) and reject AGPL `rav1d-safe`. |
| Animations | `velato` `0.12`, `charmed-harmonica` `0.2`, `animate-core` `0.8`, `simple-easing` `1.1`, `keyframe` `1.1`, `glam` `0.34` | Disable `velato` default `vello` feature to bridge `RenderSink` onto `vello 0.12`; no complete pure-Rust `.riv` runtime exists yet on `crates.io`. |
| PBR Textures | `ktx2` `0.5`, `basisu` `0.1`, `ddsfile` `0.6`, `bcdec_rs` `0.2`, `block_compression` `0.11` (`wgpu 30`), `exr` `1.74` | `block_compression` encodes BC1–BC7 via WGSL compute on `wgpu 30`; transcode ETC1S/UASTC in pure Rust with `basisu`. |
| 3D `.glb` / glTF 2.0 | `gltf` `1.4.1`, `bevy_mikktspace` `1.0`, `meshoptimizer-rs` `0.2.19` | Use `gltf::from_slice`/`import_slice` on `wasm32`; inspect `KHR_texture_basisu` from texture extension JSON. |

## Permissive Upstream Sources & Excluded Licenses

- **Approved (Tier A / Zero-Attribution)**: Google Material Symbols (`Apache-2.0`), Lucide (`ISC`), Phosphor Core (`MIT`), Tabler (`MIT`), RemixIcon pinned `<= v4.8.0` (`Apache-2.0`), Kenney (`CC0-1.0`), OpenClipart (`CC0-1.0`), Poly Haven HDRIs/Textures/Models (`CC0-1.0`), `ambientCG` (`CC0-1.0`), Quaternius (`CC0-1.0`), UseAnimations (`MIT`), Smithsonian Open Access (`CC0-1.0`), Wikimedia Commons CC0 WebM (`CC0-1.0`).
- **Approved with Attribution Propagation (Tier B)**: Google Noto Animated Emoji (`CC-BY-4.0`), Blender Studio Open Movies (`CC-BY-3.0`/`4.0`), Khronos `glTF-Sample-Assets` filtered via `REUSE.toml` (`CC0-1.0` & `CC-BY-4.0` subsets only).
- **Excluded / Quarantined**: `Remix-Design/RemixIcon >= 4.9.0` (custom *Remix Icon License v1.0* forbidding library redistribution), Unsplash, Pexels, Pixabay, Mixkit, Coverr, Lordicon, ShareTextures, SVG Repo default license, and Khronos `-NC`/EULA/SCEA models.
