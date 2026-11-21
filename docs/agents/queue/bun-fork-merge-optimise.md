# bun-fork-merge-optimise

Goal (user, French): merge the newest Bun canary into `aphrody-labs/bun`, bump it, make Bun the core of the product, optimise it for our needs.
Sub-goals: install the built fork as default `bun` (fr1, PC), pin fw/aphrody tooling to its version, Bun.Image gaps (`--check`, raw RGBA I/O, crop, composite, cover, WebP effort), and an automatic upstream-sync pipeline.

## Facts checked live (2026-11-21)

- The `canary` tag of oven-sh/bun is stale (0d4b4c4942, 2022). Canary is `main`: head 98eae327f2, `package.json` 1.4.6, release bun-v1.4.5 (2026-11-19).
- Fork checkout on fr1: `~/src/bun` (blob:none clone, remotes `origin` = aphrody-labs/bun, `upstream` = oven-sh/bun). No checkout on the PC.
- `bun --check <file>` already exists on main with another meaning: it type-checks first and then RUNS the file (`--check` = type check before running, and `bun check <file>` alone never runs it). Verified on fr1 with `~/.local/bin/bun`: `bun check ok.js` prints "No type errors in 1 file" and does not run it; `bun --check bad.js` exits 1 without running; `bun --check ok.js` runs it. So gap (a) is covered by `bun check <file>`. oven-sh/bun#30981 (non-executing `--check`) conflicts with that meaning; it was not merged.
- Our fork main already had `Bun.Image#raw({ channels })` (PR #49107). Upstream main has no `raw`.

## Done (verified on origin)

- `6b1aa57b99` on origin/main (aphrody-labs/bun) = merge of upstream main 98eae327f2 + the upstream-sync pipeline. Its first parent `86d5c38b06` is the verified tree. Chain:
  - `58be8ddebf` Merge oven-sh/bun main (98eae327f2): conflicts in `Cargo.lock` (take upstream, re-add `bun_migrate`), `which.rs` (take upstream, keep `crate::shell::which_command`), `rm.test.ts` (keep both).
  - `5f440fd25d` native.yml bootstrap and bench use bun-v1.4.5.
  - `d02e2f7e5c` archive/zip: inflate with `bun_deflate`, crc32 with `bun_highway` (upstream replaced libdeflate_sys and zlib_sys).
  - `86d5c38b06` FileReader `js_fd` reads `reader.get_fd()` (upstream dropped the `fd` cell).
- Evidence: debug build `bun run build` completed on fr1 (`build/debug/bun-debug --revision` printed `1.4.6-debug+5f440fd25`); `bun-debug check ok.js` printed "No type errors in 1 file".
- `476dad4f68` scripts/aphrody/upstream-sync.ts + test + README + systemd units (worker). Tests: 34 pass on fr1. Dry run live: 3460 open PRs (draft 356, not-mergeable 1317, waiting-ci 343, red 712, candidates 733), 984 discussions (48 flagged untracked). Timer is NOT installed (`systemctl --user list-timers` has no bun-upstream entry).
- GitHub Actions `native` run 44863168510 started on 6b1aa57b99 (linux-x64 musl and gnu release builds, then prerelease `native-<date>-6b1aa57b99` with zips and SHA256SUMS). Check it: `gh run view 44863168510 -R aphrody-labs/bun`.

## Not done, not verified (nothing below is on origin)

1. Local-only commits in `~/src/bun` (never compiled): `1384505312` merge of oven-sh/bun#49191 (ImageData, raw pixels in/out; resolutions: `src/png/decoder.rs` kept main's global-Vec decoder and returns `Lender::adopt(vec)` so the caller copies; `jsc_hooks.rs` and `Image.rs` keep both `.raw()` and `.imageData()`), `12ed5da941` merge of #44624 (`fit: "cover"`, `position`). Bundle: `/srv/xbuild/ubuntu/bun/handoff/local-main.bundle`, list in `local-only-commits.txt`.
2. Uncommitted working tree in `~/src/bun` (patch copy: `/srv/xbuild/ubuntu/bun/handoff/unverified-working-tree.patch`):
   - `extract({ left, top, width, height })` (port of #40379 onto the new bindgen layout, error `ERR_IMAGE_BAD_EXTRACT_AREA`, tests, docs, types);
   - `composite(input | layers[], { left, top, blend: "over" | "source" })`, runs last after modulate, overlay bytes copied at call, `Arc<Vec<Overlay>>` in `PipelineTask`, `codecs::composite` + `Blend` + `Placement`, 9 tests in `test/js/bun/image/image.test.ts` ("composite" block), docs, types;
   - `src/bun_core/lib.rs`: `append_behind` generic over the allocator (needed by #49191's `Lender`; last compile error seen, fixed, not rebuilt).
3. A rebuild was not finished: the last two `bun run build` attempts died on a toolchain race (`rustc` missing while another session's rustup ran) and a SIGTERM of `cargo build -p bun_executable --lib`. Just rerun.
4. WebP effort (`webp({ lossless, effort, quality, nearLossless, exact })`): measured, not implemented in the fork. On the OpenPets ffmpeg fixture decoded to PNG (1536x2288 RGBA, 20665 colors, 88% transparent; `/tmp/chk/byron.png`, source `C:\src\openpets\packages\aphrody\test\fixtures\byron-love.ffmpeg.webp`), Bun lossless gives 356,110 B at any quality; `cwebp -lossless -z 6` 359,298; `-z 7` 260,876; `-z 9` (method 6, quality 100) 153,044. The Rust encoder (`src/webp/lossless/encoder`) is a fixed port of libwebp method 4, quality 70; the jump comes from quality >= 90 and from method 6 with quality 100 (brute-force transform and crunch configs). Worker `a0225676016395975` (clone `~/ci/bun-webp`) was told to push only verified work and report; read its notification or `git -C ~/ci/bun-webp log origin/main` before redoing anything. After it lands: plumb `effort`/`quality`/`nearLossless`/`exact` in `Image.rs` `set_format` (look at `options.get(global, "lossless")`) into `codecs::EncodeOptions` and `codec_webp.rs`, add types (`webp(options?: { quality?; lossless?; effort?; nearLossless?; exact? })`), docs, tests, and prove about 153 KB on the fixture.
5. Release builds and tests: no release binary exists yet; the `native` workflow run above produces linux-x64 gnu/musl. Windows: dispatch `gh workflow run aphrody-windows.yml -R aphrody-labs/bun --ref main` (default test list included). Core test subsets not run: `bun bd test test/js/bun/{shell,spawn,http,sqlite,image} test/cli/run test/cli/check`.
6. Install/pin (nothing changed yet): replace `~/.local/bin/bun` on fr1 and `D:/cache/bun/bin/bun.exe` on the PC only after `bun --revision` plus `bun test` in `C:\src\openpets\packages\pet-format` pass (keep the old one as `bun-upstream`/`bun-upstream.exe`). Pins found: `fw/packages/ai-os/engines/pi-opencode/package.json` `"packageManager": "bun@1.4.4"`; `ay/packages/org-mirror/test/manifests.test.ts` mentions `bun@`; aphrody `.github/workflows/*.yml` use `oven-sh/setup-bun@v2` without a version; `fw/.github/workflows/modern-web-sync.yml` has `bun-version: latest`.
7. Upstream-sync timer: `bun scripts/aphrody/upstream-sync.ts install` on fr1 (unit `bun-upstream-sync.timer`, logs in `~/ci/bun-upstream/log`, reports `~/ci/bun-upstream/report-<ts>.md`), then one real run. The first real run gates PRs with `bun bd`, so it needs a working `~/ci/bun-upstream/clone` build; keep `--max-prs` low.
8. Before/after numbers the user wants (binary size, startup ms, bench mean and spread): no measurement taken yet. Baseline upstream canary on fr1: `~/.local/bin/bun` 1.4.6-canary.1+98eae327f.

## Resume

```
ssh fr1
cd ~/src/bun && git status --short && git log --oneline -4          # local-only merges + working tree described above
git fetch origin && git merge origin/main                           # origin/main = 6b1aa57b99 (+ webp worker commits)
bun run build                                                       # debug build, output on /srv/xbuild (build -> symlink); BUN_DEBUG_QUIET_LOGS=1 for quiet
BUN_DEBUG_QUIET_LOGS=1 bun scripts/build.ts --profile=debug && build/debug/bun-debug test test/js/bun/image
bun run build:release                                               # needs ~15 GB; put --build-dir on /srv/xbuild/ubuntu/bun
```

If the working tree is gone: `git apply /srv/xbuild/ubuntu/bun/handoff/unverified-working-tree.patch` after `git fetch /srv/xbuild/ubuntu/bun/handoff/local-main.bundle` and merging `12ed5da941`.
Do not run `cargo`/`rustup` in a checkout with `rust-toolchain.toml` without `RUSTUP_TOOLCHAIN=nightly-2026-11-14`: rustup tries to install 12 targets and removes the toolchain while another session builds.

## Directories created (fr1)

- `/srv/xbuild/ubuntu/bun` 29 GB: `build/` 19 GB (debug build tree, incremental cache; `~/src/bun/build` is a symlink to it; keep until the next build is done, then safe to delete), `cache/` 9.4 GB (WebKit prebuilt and dep cache from `~/build/cache`, safe to delete, refetched), `handoff/` 180 KB (keep).
- `~/ci/bun-upstream` 1.0 GB: `dev-clone` (pushed, idle, safe to delete), `log`, two dry-run reports (keep or delete).
- `~/ci/bun-webp` 1.0 GB: webp worker clone; delete only after its report and push.
- `~/build/glibc-sysroot.sh`, `glibc-sysroot.log`, `bun-debug.log` (small). `/opt/linux-sysroot-glibc` was empty of libc headers and was repaired with the generated sysroot script (x64 only); `/opt/linux-sysroot-glibc-arm64` untouched.
- Removed on fr1 to free space: `~/.cache/uv` (6.4 GB cache) and `~/src/fw/target/debug/incremental` (12 GB, cargo rebuilds it).
- Rust: `nightly-2026-11-14` re-installed with the minimal profile plus `rust-src`, `llvm-tools` (the full-profile one was removed by a rustup rollback when the disk filled).

No tmux jobs or build processes of this task are left running; the `native` GitHub Actions run is the only thing in flight.
