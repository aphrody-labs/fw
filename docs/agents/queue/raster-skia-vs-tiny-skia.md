# Handoff: raster benchmark, Skia (Chromium 160 pin) vs tiny-skia (Obscura)

## Goal
In `aphrody-labs/gpu-bench` under `raster/`: typed Bun runner `raster/run.ts` + `raster/RESULTS.md` comparing Skia and tiny-skia CPU rasterization
(paths, gradients, images, 29 blend modes, clips, text, M3 screen, Byron sprite sheet, Obscura page render), at 1920x1080 and 2x DPR, with p50/p95,
RSS, sizes, build time, PSNR/max delta/diff images, feature gaps, and a numeric conclusion (keep tiny-skia vs adopt Skia or Vello). Commit and push, no AI trailers.

## Decisions already made (verified live this session)
- Chromium 160 is not branched (branch point 2026-11-30 per chromiumdash `fetch_milestone_schedule?mstone=160`). `chromium/src` main is 160.0.8175.0 and its DEPS
  `skia_revision` is `67a5ceb26d828f46155b6ea2fa9afc510c5aa8b3` = `aphrody-labs/skia` main = the submodule pin. M159 branch pin is `b809e72312b060cfa2d865369b434840cf6c7538`. State this in RESULTS.md.
- `$WORKSPACE_ROOT/browser` is not a Chromium checkout (no chrome/VERSION), so nothing was read from it.
- tiny-skia 0.12.0 has sweep (conic) gradients, all 29 blend modes, Nearest/Bilinear/Bicubic, no mipmaps, no blur, no text. Obscura draws glyphs via cosmic-text SwashCache + scalar source-over, replicated in `raster/rust/src/bin/bench.rs`.
- fr1 load average was 70 to 130 (shared): harnesses report thread CPU time (`cpu_times_ms`) next to wall time; report uses CPU time. Re-check `uptime` before the real run.
- fr1 `/` is nearly full: all fr1 work lives under `/srv/xbuild/raster`.

## Done (on origin/main of gpu-bench)
- `c51714a` submodules `vendor/skia` (67a5ceb26d) and `vendor/obscura` (0d9a2f764f), relative URLs, shallow.
- `d03326b` verified: `raster/scenes.ts` (50 scenes; `bun raster/run.ts prep` ran OK on PC and fr1), `raster/assets/*` (tokens snapshot, Byron plan + 8 webp sources), `raster/rust/*` (`cargo check` clean for `bench-tiny` and `page-obscura`; Cargo.lock seeded from Obscura), `raster/skia/gn-args.txt` (Skia CPU-only build: 1210 ninja steps, 958 s under load ~100 on fr1, libs produced: libskia.a 12.8 MB, libskshaper.a 8.8 MB, libicu.a 22 MB, libharfbuzz.a 8.6 MB).
- `7129444` NOT verified (WIP): `raster/run.ts`, `raster/util.ts`, `raster/report.ts`, `raster/skia/bench.cpp`. `bench.cpp` was never compiled. `run.ts build` last failed on parsing `gn desc` JSON (a "WARNING" line precedes the JSON); a fix (slice from first `{`) was applied locally in `run.ts` line ~214 and is in the commit but untested. Note it calls gn desc twice (wasteful, harmless).

## Remaining
1. fr1: `cd /srv/xbuild/raster/gpu-bench && git fetch && git merge origin/main` (or re-tar raster/), then `bun raster/run.ts build --rebuild`; fix compile errors in `bench.cpp` (Skia API of 67a5ceb: SkPathBuilder, SkGradient, SkShapers::HB, SkFontMgr_New_Custom_Data) and link flags. `work/build.json` is pre-seeded with the 958 s Skia build time.
2. `bun raster/run.ts bench` (50 scenes x 2 DPR x 4 engines, `--runs 20`), then `diffs`, `pages` (Obscura page harness needs cargo features `page`; pages from `$WORKSPACE_ROOT/aphrody/site/public/*.html`, copy them to `raster/assets/pages/<name>.html` or set `RASTER_PAGES` on fr1 since that path does not exist there), `report`.
3. Write `raster/RESULTS.header.md` (method, Skia pin note above, host) and `raster/RESULTS.conclusion.md` (numbers only from results JSON), run `report`, commit `raster/results/*.json`, diff PNGs (worst 10 only) and RESULTS.md.
4. Windows numbers: optional. Rust targets build with `cargo build --target x86_64-pc-windows-msvc`; Skia cross-build not attempted. `getrusage`/thread CPU are unix-only in the harnesses (Windows would report wall time and RSS 0).
5. Vello: not benchmarked; add `vello_cpu` only if time allows, otherwise say "not measured" in the conclusion.
6. Known gaps in run.ts: `bench()` drops `pages` from the results JSON when rerun; PSNR of text scenes will be low (hinting/AA differ), expected.

## Resume commands
```
ssh fr1
cd /srv/xbuild/raster/gpu-bench && export PATH=$HOME/.local/bin:$HOME/.cargo/bin:$PATH TMPDIR=/srv/xbuild/raster/tmp
git fetch && git merge origin/main      # tree here is a clone whose raster/ was overwritten by tar; commit nothing from fr1
bun raster/run.ts prep && bun raster/run.ts build --rebuild
bun raster/run.ts bench && bun raster/run.ts diffs && bun raster/run.ts pages && bun raster/run.ts report
```
Copy `raster/results/` and `raster/RESULTS.md` back to the PC tree (`$WORKSPACE_ROOT/gpu-bench`) and commit there by path; before each push `git fetch` + `git merge origin/main`, never rebase.

## Directories created
- fr1 `/srv/xbuild/raster` 1.5 GB (Skia checkout with synced deps, `out/Release` build, `gn-args.txt`, build logs; `gpu-bench/raster/work`, `/srv/xbuild/raster/tmp`). Keep until the bench is done; then safe to delete.
- fr1 `~/raster-bench` was moved to the above and removed. Nothing left in the fr1 home. Large `third_party/externals` entries (unicodetools, swiftshader, emsdk, dawn, ...) were deleted on purpose (CPU-only build does not need them; `run.ts` does the same trimming after git-sync-deps).
- PC `$WORKSPACE_ROOT/gpu-bench/vendor/skia` 205 MB, `vendor\obscura` 33 MB (submodules, keep), `$WORKSPACE_ROOT/gpu-bench/raster/work` 25 MB and `raster\rust\target` (cargo check artefacts, regenerated; both gitignored, safe to delete).
- No long jobs of mine are running (build-run exited with the gn desc error; checked with pgrep on fr1).
