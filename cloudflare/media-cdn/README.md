# media.aphrody.com

The media CDN of the monorepo: images, video and SVG sprites from one R2 bucket (`aphrody-media`) behind one Worker (`media-cdn`). Fonts and icons stay on `material.aphrody.com` (`cloudflare/material-cdn`). The design is derived from a live measurement of `googlebook.google`, recorded in `plugins/aphrody/skills/design/references/google-design.md`; the requirements are `docs/product/cahier-des-charges.md` (R1 to R7). `GET /v1/` answers with the routes and the manifest revision as JSON.

## URLs

The grammar lives in `tools/media-cdn/grammar.ts`, shared by the Worker, the builder and the verifier.

| URL (below `/v1/`) | Result |
| --- | --- |
| `img/<name>[.<sha8>][.<fmt>][=<tokens>]` | An image. `fmt` is `avif`, `webp`, `jpg` or `png`; without it the `Accept` header chooses AVIF, then WebP, then the fallback (JPEG for opaque images, PNG for alpha) and the answer carries `Vary: Accept`. Tokens are joined by `-`: `w<px>`, `h<px>`, `s<px>` (long edge, aspect kept), `q<30..95>`; `s` cannot be combined with `w` or `h`. Sizes are 16 to 4096 and never above the source |
| `video/<name>[.<sha8>]/<600\|1024\|1440\|1920>w-<1\|2>x-<av1\|vp9\|h264>.<mp4\|webm>` | One rung of the video ladder. `<D>x` is the pixel density: the file is `D` times as wide in pixels as the CSS width. Range requests answer 206 |
| `video/<name>[.<sha8>].json` | The `<source>` rows in selection order (per breakpoint: AV1 2x, VP9 2x, H.264 1x), with `type` including `codecs=`, `media` queries for 600, 1024 and 1440 CSS px, absolute URLs |
| `sprite/<name>[.<sha8>].svg` | One SVG with a `<symbol id>` per source file |
| `manifest.json` | Every name with hash, size, rungs, bytes, codec strings |

`<sha8>` is the first 8 hex digits of the content hash. A pinned URL is immutable and stays valid after a republish (older objects are never deleted); an unpinned one follows the manifest.

Examples (names come from `manifest.json`):

```
/v1/img/hero.avif=w1440            AVIF, 1440 px wide
/v1/img/hero=s144                  best format for the browser, long edge 144 px
/v1/img/hero.3f9a2c1d.webp=w600    pinned, immutable
/v1/img/hero.avif=w500-h300        cover crop to 500x300, made on demand
/v1/video/intro.json               source list for <video>
/v1/sprite/ui.svg                  the sprite
```

A sprite is referenced with `<use href="#id">` after it is inlined once in the page (browsers refuse cross-origin `<use>`); `material.aphrody.com/v1/icons.svg?icons=` builds the subset for the Material icons.

## How it serves

- Prebuilt rungs: widths 320, 600, 1024, 1440, 1920, 2880, 3840 and the native width, long edges 48, 72, 96, 144, 192, 512 (only those not above the source), each in AVIF, WebP and the fallback. They are plain R2 reads.
- Other sizes, `h` and `q`: one transform of the stored source by the Images binding, kept in R2 under `v1/gen/<hash>.<fmt>` (hash of algorithm, source hash, format and canonical tokens) and read from there afterwards. `X-Media-Source` is `r2` or `built`.
- Headers: pinned names `public, max-age=31536000, immutable`; names that follow the manifest `max-age=300, s-maxage=3600, stale-while-revalidate=86400`; ETag and conditional requests (304); `Accept-Ranges: bytes`; `Access-Control-Allow-Origin: *`, `Timing-Allow-Origin: *`, `Cross-Origin-Resource-Policy: cross-origin`, `X-Content-Type-Options: nosniff`; SVG with a sandboxing CSP. Text is compressed by the Cloudflare edge; AVIF, WebP, JPEG, MP4 and WebM are not.
- Bindings (`wrangler.jsonc`): R2 `MEDIA` for `aphrody-media`, Images `IMAGES`. There is no route in the file: the hostname is attached by the estate (`cluster/estate.toml`).

## Build, publish, verify

Encoders run on the cluster nodes (`fr1`, `prd`), never on the PC.

```sh
bun tools/media-cdn/fixtures.ts --out DIR                       # synthetic test sources (our own content)
bun tools/media-cdn/build.ts --src DIR --out OUT [--jobs N]     # DIR/img, DIR/video, DIR/sprite/<name>/*.svg -> OUT/v1/...
CLOUDFLARE_API_TOKEN=... bun tools/media-cdn/publish.ts --out OUT   # changed objects only, manifest last, merged over the bucket's
bun tools/media-cdn/verify.ts [--host https://media.aphrody.com]    # bytes, headers, magic numbers, negotiation, ranges, on-demand
cd cloudflare/media-cdn && bun test                              # grammar and Worker against an in-memory bucket
cd cloudflare/media-cdn && bun run deploy                        # wrangler deploy
```

`build.ts` needs `ffmpeg` (libsvtav1, libvpx-vp9, libx264), `ffprobe`, `vips`, `avifenc`, `cwebp` and `magick`. Video: AV1 crf 34 (SVT-AV1 preset 8), VP9 crf 36 (cpu-used 4), H.264 crf 24 (preset slow), 30 fps source rate kept, no audio, yuv420p, `faststart` MP4; a rung is skipped when the source is narrower than its pixel width. A poster frame is published as the image `<name>-poster`.
