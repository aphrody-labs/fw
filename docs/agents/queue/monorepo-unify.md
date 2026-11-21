# Monorepo merge into fw: state and exact blockers (2026-11-21)

aphrody and ay are inside fw (owner decision: fw is the monorepo). Where things are, what was measured, what is open.

## On fw main

Archived on 2026-11-21 with the owner's go-ahead (pointer commits first): aphrody-labs/aphrody (`b96b124586aa9f2a14fc2c29f3ec1f61a3c26027`) and aphrody-labs/ay (`ddf621a9224fc61529541175d17508c6416accd6`). The edge deploy runs from fw: `ay-edge-deploy` run 44891875280 (verify and deploy success), repository variables EDGE_DEPLOY and EDGE_CUTOVER_DONE set on fw, `https://releases.aphrody.com/manifest.json` and `https://aphrody.com/` answer 200.

| step | commit |
| --- | --- |
| import with full history under `aphrody/` and `ay/` (filter-repo; fw 390, aphrody 41,072, ay 256 commits, 41,721 after the merge) | `70e6143d8235778a12c446767db5d9d05ea0bc8b` |
| original aphrody history recorded as merged (filter-repo rewrote Zed's commit ids, so there was no merge base with zed-industries/zed; this tree-neutral merge makes `git merge-base HEAD upstream/main` resolve) | `9b621306c0e676d5d80f015426fd428c5b4b2c8e` |
| pure-Rust guard (baseline 1,970 non-Rust source files) | `e1066c806c4ab05fb844b22c36e976f4f6e6a904` |
| one Bun workspace (`ay/packages`, `ay/apps` join; `bun install --frozen-lockfile` passes) | `1a14ed1aeecb18cabf1e4c5db2939d1aad3feae2` |
| one Cargo workspace (unify.ts on current mains), one Gradle build (`kotlin/`, `ay/kotlin` included), release train at the root, deny policy, purity, ported CI | `1c6f15a870c5f473350d33c896ec5ba5b4af983d` |
| remaining ay workflows ported, nested `ay/.github/workflows` and `actions` forbidden by the layout check | `8fbf315db98a855dae33a1d29d2b918595730dd5` |
| aphrody commits made after the import merged through `-X subtree=aphrody` | `97378dfe0aacfa8b7d9f4f1c5b0a19be610dccff` |

## Gates measured on the merged tree (host fr1, target /srv/xbuild/target/merge)

- `cargo check --workspace --all-targets --keep-going --locked`: 0 errors, Finished in 7m22s (after porting aphrody `util::fs_embed` and `nsh-bundler` to rust-embed 8.14 and oxc 0.158, the handoff's `983ca6bf15`; before the port 9 units failed: assets, grammars, settings, util (lib test) and nsh-bundler).
- wasm gate `cargo check -p aphrody_desktop --features web --example web_os --target wasm32-unknown-unknown`: Finished in 3m19s.
- `cargo deny check`: advisories, bans, licenses, sources ok (one policy; aphrody's license list, git sources and the advisory ignores added with reasons; the license list now allows GPL-3.0 and LGPL-3.0 families mechanically: the licensing question is the owner's, not decided here).
- `cargo xtask purity`: pass; `cargo xtask map` regenerated.
- Gradle `gradlew test` in `kotlin/` (fw-core, fw-material3, fw-rust-ffi and the included `ay/kotlin` theme): BUILD SUCCESSFUL in 3m46s.
- `bun test` of release-plan and release-publish: 19 pass, 0 fail.
- `cargo nextest run --workspace --no-fail-fast`: not finished at the time of writing (the test build of 380 crates was still compiling after 22 minutes on a host at load 50 to 65); result goes below when it lands.

## Open, with the exact cause

1. fw-files SFTP: still out of the workspace (crates.io `rsa` 0.10.0 features are default, encoding, getrandom, hazmat, pkcs1v15, pkcs5, serde, std: none has `sha2`, while quickjs-jit-stdlib 0.14.2 enables `rsa/sha2`; it only resolved on a release candidate range that russh 0.65.2 cannot share) (`unify.toml` strips `sftp`, `russh`, `yazi-sftp`). Re-tested today: restoring the 0.65.2 manifest of `5a4e7d79` fails with `failed to select a version for rsa ... package quickjs-jit-stdlib depends on rsa with feature sha2 but rsa does not have that feature` (quickjs-jit-stdlib 0.14.2, the newest on crates.io, nsh-js crypto). The fix is a patch of quickjs-jit-stdlib (aphrody-labs fork of longbridge/quickjs-jit without the `sha2` feature request) or a release of it against rsa 0.10.0.
2. libsqlite3-sys lane: the unified workspace stays on libsqlite3-sys 0.38 because `bun-windows-winclean` 1.4.8 (aphrody-labs/bun, `packages/os/bun-windows-winclean`) pins `rusqlite ^0.40` and `ort =2.0.0-rc.12`; rusqlite 0.43 fails with `package libsqlite3-sys v0.40.0 ... conflicts with a previous package which links to sqlite3` (fw-deps lane, below).
3. ay-vault: out of the workspace (`age 0.12.1` needs `ml-kem ^0.2` and `kem =0.3.0-pre.0`; the mullvad fork behind aphrody_vpn needs ml-kem 0.3.4).
4. fw-rag oxc: the workspace is on oxc 0.158 because rolldown 1.3.2 needs `^0.158`; oxc 0.159.0 is the newest (fw-deps).
5. zip 8.6.0 via aphrody-wireguard 1.4.7 <- aphrody-remote-desktop 1.4.7 (fw-agent feature `tools`): five Dependabot alerts; the fix is publishing both against zip 9. glib 0.18.5 via nsh-platform-gpui -> wry 0.57.3 -> webkit2gtk 2.0.2 (gtk3-rs has no glib >= 0.20 release).
6. The 15 aphrody units that failed `cargo check --all-targets` on their own are fixed (aphrody commits `f4242f42ad18077a5e3c078cf7b2db6a7513b07b`, `29f3327f31e007ab06e301252ef8d5d25c1c3bb1`, `570a1440505bc04e3379fc365261576093c4a284`, all merged into `aphrody/`); no unit is excluded; the unified check has 0 errors. What still fails is test execution (nextest of the fixed units, measured by the units agent on fr1):
   - gpui-base and gpui-component: 17 failed and 1 timed out of 3186. 12 are retained/dirty-view tests that need the GPUI Fast retained-rendering backend, which the unified `aphrody/crates/gpui` does not have; the rest are `theme::schema::test_aurora_theme_parses_gradient_backgrounds`, the two `block_surface` tests, `table_row_backgrounds_follow_the_frame_corner_radius`, `editable_input_offers_accessibility_write_action`; `table_resize_matches_cold_wrapping_at_fractional_thresholds` times out at 60 s.
   - nsh-core: `a_built_in_bar_keeps_within_the_scrollbar_inset` (expected y=10, got y=0), `keyed_list_cell_whose_row_goes_can_read_first` (got `[]`, expected `["r0","r2"]`), `the_header_shadow_reaches_the_screens_config` (panics: no reactive runtime).
   - nsh-engine: `net::closing_a_socket_ends_a_pending_read_and_write` fails in every run (`done true null false`, loopback buffer dependent); `components::the_page_shown_decides_the_orientations` is flaky (failed in two of three runs).
   These are the nextest blockers of the merged workspace until fixed or the retained-rendering backend is ported.
7. Workflows: all nine remaining aphrody workflows are ported to the root; seven stay manual (`workflow_dispatch` only, former triggers kept as comments: fleet-orchestrator, forks-packages, issue-fix, janitor, kernel-watch, m3-compose-tracker, wiki-sync, which would overwrite the fw wiki), auto-merge and the daily `aphrody-upstream-sync` (nsh, gpui-kit, bun, agent targets; the hourly Zed part is `zed-sync.yml`) keep their triggers. `aphrody/.github/workflows` is deleted and forbidden by the layout check; repo-hygiene, toolchain-audit, ci-host-optimize and tooling.test.ts read the root workflows (tooling.test.ts: 6 pass). Still failing in `aphrody/tooling/bun/toolchain-audit.test.ts`: 3 tests read `aphrody/Cargo.toml`, which the unification removed (they need the root manifest and counts of the merged workspace).
8. Secrets the ported workflows name that are not set on fw (names only; they are not set on the old repositories either): FLEET_SSH_KEY, FLEET_SSH_CONFIG_B64, FLEET_BEACON_TOKEN, APHRODY_WORKER_URL, APHRODY_APP_PRIVATE_KEY (variable APHRODY_APP_ID), FLEET_DEPLOY_KEY, AUTO_MASTER_KEY, variables EDGE_DEPLOY and EDGE_CUTOVER_DONE. Set on fw: CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_API_TOKEN, R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY (organization level: APHRODY_APP_KEY, CARGO_REGISTRY_TOKEN, HOSTINGER_API_TOKEN, NPM_TOKEN, SUBMODULES_APP_KEY).
9. Zed subtree sync: mechanism in place (`zed-sync.yml` hourly; `git merge -X subtree=aphrody upstream/main`; resolver ported to the prefix in `aphrody/tooling/bun/upstream-sync.ts`: excised files dropped, README and isolated blocks keep Aphrody's side, Cargo manifests keep the root side and the upstream side is saved under `target/zed-sync`). A real `zed/main` merge has not landed yet: upstream main had no commit since 2026-11-21 00:49 UTC (behind 0). A test merge of `upstream/v0.234.x` (18 commits not in aphrody) found the merge base `b7d35e528a` and reported 23 changed paths with 20 conflicts, which is a lagging release branch against a main-following fork, not a main merge. The first hourly run with a new upstream commit is the proof.
10. Root `src/` and `tests/` (TypeScript) into `packages/fw`: blocked by package.json scripts, the plugin runtime and MAP.tsv rows (a pure-Rust port removes the code instead).

## Earlier handoff (phase 1 checkpoint, before the import)

# Monorepo phase 1: one Cargo workspace (handoff)

Task: one root workspace for fw, aphrody and ay (members `crates/*`, `aphrody/crates/*`, `ay/crates/*`), every dependency at its newest release, conflicts resolved, gates green. Stopped at a checkpoint: the workspace builds and resolves, nothing of it is on main yet because main would go red on aphrody's own pre-existing breakage (below).

## Pushed to fw main

- `4bd1526f` `tools/monorepo/unify.ts` and `unify.toml`: the re-runnable transformation (manifests in, one workspace out) with every decision and its reason; a later commit updates both (profile merge, async-tungstenite).
- Earlier in the same lot: `edd22a2` deny.toml, `62966c5d` unused dependencies, `60b560da` catalog, `5c2dea08` fw-secrets tokio fs, `ec76a3b8` rust-audit and the rust-hygiene CI job (green on main), `51b9c9e6` gpui row.

## Scratch (not pushed): `/srv/xbuild/merge/fw`

Commits on top of the import `4edfee2d37`: `983ca6bf15` (aphrody util::fs_embed and nsh-bundler ported to rust-embed 8.14 and oxc 0.158), `a1e12f6c9a` and `65679d403a` (the unified manifests, lock and config). The scratch is based on fw `cbbb3316`; to land, redo the import on current main (`bun tools/monorepo/import.ts`, which needs a scratch that can fast-forward, so start from a fresh `/srv/xbuild/merge`), run `bun tools/monorepo/unify.ts`, re-apply the two aphrody source ports, then the gates.

## Measured (fr1, host load 2 to 5 when measured)

- Locks before: fw 1,229 packages, aphrody 2,552, ay 834 (4,615 entries, 563 names duplicated in 1,301 lanes over the union). After: one lock of 3,065 packages, 2,633 distinct names, 311 duplicated names in 730 lanes.
- `cargo check --workspace --all-targets --keep-going`: 3 m 00 s with a warm sccache and target (8 m 25 s on the first, partly cold run); aphrody alone before the merge: 10 m 36 s.
- Failing units: the unified workspace fails exactly the 15 units that aphrody fails on its own (checked by running the same command on the imported aphrody tree): gpui-component (lib, `vertical_scrollbar` on `Observed<E>` once gpui test-support is unified in), gpui, gpui-base, gpui-component, nsh-core, nsh-macros, nsh-bundler, nsh-platform-gpui (lib tests), nsh-cli (bin, `CodeSectionStart.unchecked_range`), nsh-js (tests standard and web: `tests/standard.rs` modules resolve beside it), nsh-platform-web, nsh-platform-tui, nsh-engine (tests; `aphrody/tests/compat` fixtures and a gpui example jpg are not in the repository). No unit fails that did not fail before.
- `cargo nextest run` over the fw and ay members (110 binaries): 3,039 tests, 3,038 passed, 1 failed (`fw-secrets composite_falls_through_to_second_store`, in a scratch based on a fw commit before `5c2dea08`; fw-secrets builds alone only after that fix). Aphrody's tests were not run (its all-targets build does not pass on its own).
- Clean release build of `fw-bin` on a quiet host: unified 80.3 s wall, 159.0 s CPU, binary 4,684,744 bytes; the same command at the baseline commit `ef225417`: 78.95 s wall, 155.9 s CPU, 4,667,208 bytes. The 17.5 kB come from the aphrody config the root now carries (`-C symbol-mangling-version=v0`, `--cfg tokio_unstable`); with RUSTFLAGS replaced by the old two flags the binary is 4,667,336 bytes. (The first baseline run recorded in the audit commits, 157 s wall and 73.5 s CPU, was taken on a host at load 34 and is not comparable.)
- The aphrody desktop clean release build and the wasm gate (`aphrody_desktop --features web --example web_os --target wasm32-unknown-unknown`) were not run.

## Conflicts

Solved (unify.toml carries each reason): tree-sitter (aphrody's git revision for everyone, tree-sitter-language 0.1.8 through the patch), libsqlite3-sys (one 0.38 lane, sqlez from 0.30), rolldown family at `=1.3.2` with oxc 0.158 (rolldown 1.3.2 needs oxc ^0.158; fw-rag compiles against 0.158), async-tungstenite 0.35 with jupyter-websocket-client 3.0.1, yawc pinned 0.4.2, wgpu-sync on lock_api 0.4 and parking_lot 0.12 (lock edit), per-origin versions where majors differ (ropey 1 and 2, sha2 0.10 and 0.11, ed25519-dalek, hmac, zstd, toml 0.9 and 1, etc., 72 specs in all).

Blocked by third parties (worked around by removing the smallest piece):
- libsqlite3-sys 0.40 and rusqlite 0.43: `bun-windows-winclean` 1.4.8 (published from aphrody-labs/bun, `packages/os/bun-windows-winclean`) requires rusqlite 0.40. Republish it against 0.43 and move `[spec.libsqlite3-sys]` to 0.40.
- rsa: fw-files SFTP (yazi-sftp + russh, russh at most 0.65.1 pins rsa =0.10.0-rc.18, 0.65.2 needs rsa 0.10 stable) cannot share a graph with nsh-js crypto (quickjs-jit-stdlib 0.14.2 needs rsa ^0.10.0-rc.19 with its `sha2` feature, which 0.10.0 does not have). `sftp` leaves fw-files (default features now empty) until quickjs-jit-stdlib moves to rsa 0.10.0.
- age 0.12.1 needs ml-kem 0.2 (kem =0.3.0-pre.0), the mullvad fork needs ml-kem 0.3.4: `ay/crates/ay-vault` (a leaf) leaves the workspace.
- `aphrody/tooling/xtask` is excluded because fw's `xtask` owns the package name.

Not done: windows and windows-sys lanes (not touched, they do not conflict in the lock), fontdb/read-fonts/cssparser/reqwest/hyper lanes, nix 0.25 to 0.32, and the remaining incompatible bumps (`cargo upgrade --incompatible` lists about 120 requirements behind the newest; only oxc, async-tungstenite and the conflict families were taken).

## Gate before any push

1. Decide what to do with aphrody's 15 failing units: fix them (fixtures, `Observed` bounds, nsh-cli prune.rs) or run the root CI with `--exclude` for them; `cargo clippy --workspace --all-targets` and `cargo test --workspace` in `.github/workflows/ci.yml` fail on them today, so a push would turn main red.
2. `cargo nextest run --workspace` (aphrody tests included), the wasm gate, then push path-scoped and read `gh run list -R aphrody-labs/fw --workflow ci.yml --branch main`.

## Dependency lane (fw)

Measured on fr1 2026-11-21 with `cargo upgrade --incompatible --pinned`, `cargo update --dry-run --verbose` and `cargo tree -i`. Every other requirement of the fw workspace is at its newest release (notify 9, resvg 0.50, russh 0.65.2, yazi-sftp v26.9.1-aphrody.20261121, clap, toml, url, zip, tracing-subscriber, zerocopy, walkdir, protoc-bin-vendored, tonic-prost-build; oxc 0.159.0 and craft-libs v0.1.0-aphrody.20261120.2 are the newest releases and tags).

Held back by one first-party crate, `bun-windows-winclean` 1.4.8 (aphrody-labs/bun `packages/os/bun-windows-winclean`, pulled by fw-agent feature `tools` as `aphrody-winclean`):
- rusqlite 0.40.2 (newest 0.43.0): winclean requires `rusqlite = "^0.40"`; with fw on 0.43 cargo fails with `package libsqlite3-sys v0.40.0 ... conflicts with a previous package which links to sqlite3` (winclean's rusqlite needs libsqlite3-sys `^0.38.0`).
- ort 2.0.0-rc.12 (newest rc.15) and fastembed 5.17.3 (newest 7.7.1; 5.17.4 and 6.0.0 need ort =rc.13, 7.7.1 needs =rc.15): winclean declares `ort = "=2.0.0-rc.12"` under `cfg(windows)`, and cargo resolves every target. Cargo error with ort rc.15: `failed to select a version for ort ... required by package bun-windows-winclean v1.4.8 ... versions that meet the requirements =2.0.0-rc.12 are: 2.0.0-rc.12`.
- Fix: bump rusqlite to 0.43 and ort to =2.0.0-rc.15 in `packages/os/bun-windows-winclean/Cargo.toml` of aphrody-labs/bun, publish the next version, then raise fw-model (fastembed 7.7.1, ort =2.0.0-rc.15) and the workspace rusqlite to 0.43.

Held back by first-party crates published only as 1.4.7:
- zip 8.6.0 (newest 9.8.1; five open Dependabot alerts on zip, patched in 9.0.0 and 9.0.5): `cargo tree -i zip@8.6.0` gives zip 8.6.0 <- aphrody-wireguard 1.4.7 <- aphrody-remote-desktop 1.4.7 <- fw-agent (feature `tools`, crates/fw-agent/src/tools/computer_use.rs). The zip use is `zip::ZipArchive` in aphrody-wireguard `driver.rs::install_from_archive`. computer_use.rs uses `rd::target` (tunnel inventory resolution over the embedded aphrody-tunnel.json), `rd::tokens`, `rd::vnc::client::VncSession` (vnc-rs, keymap, JPEG encode) and `rd::backend::MouseButton`; no upstream crate provides the inventory model, so replacing the crate means moving about 1800 lines (target, tokens, vnc client, keymap, encode) plus the inventory into fw-agent. Fix: publish aphrody-wireguard and aphrody-remote-desktop with zip 9 (their source is in no workspace of aphrody-labs/aphrody, aphrody-labs/ay or aphrody-labs/fw), or move that remote model into fw-agent.
- resvg 0.48.1 (second copy next to fw-material's 0.50): `bun_browser_render 1.4.8 <- bun_browser_js 1.4.8`.
- deno_error 0.7.1 (newest 0.7.3): required by `deno_core 0.412.0` and `bun_browser_js 1.4.8`.
