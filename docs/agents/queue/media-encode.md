# media-encode: the video and image ladders on real sources (queued)

Spec: `docs/product/cahier-des-charges.md` R1 to R4, G10 and G11. Roadmap: M5 first half (done), M6 second half.

## Goal
Every product video and image is encoded by `tools/media-cdn/build.ts` on `fr1` or `prd` from a real source, with a recorded quality check per rung, and published to `aphrody-media`.

## Done
- `tools/media-cdn/build.ts` encodes AV1 (libsvtav1 crf 34), VP9 (libvpx-vp9 crf 36) and H.264 (libx264 crf 24) at 600, 1024, 1440 and 1920 CSS px, 1x and 2x, never upscaled, plus a poster; images in AVIF (avifenc q60), WebP (cwebp q80) and JPEG or PNG.
- Tools present on `fr1` (ffmpeg 8.1.2, avifenc, cwebp, vips, magick) and `prd` (ffmpeg n9.0.2, vips, SvtAv1EncApp, x264; no avifenc or cwebp: image encoding runs on `fr1`).
- Test sources: `tools/media-cdn/fixtures.ts` (synthetic).

## Remaining
1. Real sources: the aphrody hero clip and the fw session captures, recorded from our own software (never Google's clips); put them under `$WORK/media-src` on `fr1`, not in git (hashes in the manifest).
2. Quality gate: VMAF (ffmpeg `libvmaf`) of each rung against the source, written into the manifest entry; refuse a rung below the threshold set from the first measured run.
3. Encoder settings tuned from measured bytes per rung against the budgets G5 to G7; record the table here.
4. Image formats: compare avifenc speed 6 against speed 4 on bytes and time, keep the cheaper at equal SSIM.
5. A `media.yml` workflow on the cluster runners: build changed sources, publish, verify (`tools/media-cdn/verify.ts`).

## Proof
Manifest rungs with VMAF values; `bun tools/media-cdn/verify.ts` green against the live host; per-rung byte table below the G5 to G7 budgets.

## Resume
`ssh fr1`, `cd /srv/xbuild/media-cdn`; `bun src-tree/tools/media-cdn/build.ts --src <dir> --out out --jobs 2`; publish from a host that has `CLOUDFLARE_API_TOKEN`: `bun tools/media-cdn/publish.ts --out <out>`.
