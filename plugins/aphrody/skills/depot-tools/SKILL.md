---
name: depot-tools
description: "depot_tools and AOSP repo in Rust: fw depot. Use for Chromium, V8, WebRTC or Android checkouts, pinned gn, ninja, siso"
---
> (gclient sync, revinfo, getdep, setdep, runhooks, fetch, CIPD, autoninja, git cl, presubmit, repo, lunch). , clang-format and Rust toolchain installs, DEPS and manifest analysis

> `fw depot` replaces the Python depot_tools (pinned ec77017, the revision Chromium 160.0.8167.0 uses) and Google's `repo` for the commands below. Code: `crates/fw-depot` (library plus the `fw-depot` binary that `fw depot` runs). ay consumes it through `ay-build`.

# depot-tools (fw depot)

Build and test on `fr1`, never on the PC: `cargo build --release -p fw-depot --bin fw-depot` in `/srv/xbuild/src/fw`. Reference tools for parity runs: `/srv/xbuild/src/depot_tools` (git, ec77017) and `/srv/xbuild/src/git-repo`. Caches: `FW_DEPOT_CACHE` (default `~/.cache/fw-depot`), set it to `/srv/xbuild/cache/depot` on fr1.

## Command map

| depot_tools or repo | fw depot | Parity (this session, fr1) |
| :-- | :-- | :-- |
| `gclient revinfo` | `fw depot revinfo [--filter X] [--output-json F]` | equal: Chromium 160.0.8167.0 (250 lines, 6 recursed DEPS) and a multi-OS config (363 lines: target_os android/ios/win/mac, target_cpu arm64, `custom_deps` removal and addition); gclient 12.5 s and 24.3 s, fw 0.29 s |
| `gclient sync` | `fw depot sync [--nohooks] [--no-history\|--partial] [--revision p@r] [--jobs N] [--dry-run] [--no-cipd] [--no-gcs] [--only P]` | equal on 3 fixtures (git HEADs, file trees, hook outputs, gn args file, `.gclient_entries`, CIPD files); gclient 2.5, 2.8, 4.5 s, fw 0.3 s |
| `gclient runhooks` | `fw depot runhooks` | same hook order, cwd and conditions as `sync` |
| `gclient getdep` / `setdep` | `fw depot getdep\|setdep (-r path[@rev] \| -r path:pkg@ver \| --var k[=v]) [--deps-file F]` | 32 of 32 equal on Chromium's DEPS (edited files byte equal); gclient 32.1 s, fw 2.6 s |
| `fetch` | `fw depot fetch NAME [--nohooks] [--no-history] [--dry-run] [--prop=v]` | 52 of 52 `.gclient` specs equal (all 47 `fetch_configs` plus property cases); `fetch.py` 54.9 s, fw 0.9 s |
| `cipd ensure` | `fw depot cipd (resolve\|ensure\|install)` | 2221 files, contents and modes equal (gn, ninja, siso, cpython3); cold 2.75 s vs 6.05 s, warm 0.17 s vs 0.13 s |
| the DEPS-pinned tools (gn, ninja, siso, clang-format, Rust) | `fw depot tools --chromium TAG [--root DIR]` | Chromium 160.0.8167.0: gn 2627, ninja 1.12.1, siso, clang-format 23.0.0git, rustc 1.101.0-nightly installed in 30.8 s; GCS archives byte equal to GNU `tar -xJf` (7443 files) |
| `autoninja` | `fw depot autoninja [--print] [args]` | 80 of 80 equal (Siso or Ninja choice, `-j` conversion, `.sisoenv`, `.sisorc`, `SISO_PATH`, state-marker errors); 11.75 s vs 0.81 s |
| `git cl issue\|status\|upload\|land\|patch\|description\|abandon` | `fw depot cl ... [--dry-run]` | tested against a local Gerrit-like server (`tests/gerrit_fake.rs`), not against `git_cl.py` (no Gerrit account); nothing is ever uploaded in tests |
| `git new-branch`, `git map-branches` | `fw depot new-branch`, `map-branches` | behaviour only |
| `presubmit_canned_checks` | `fw depot presubmit canned [--base REV]` | 17 of 17 findings equal (tabs, trailing whitespace, CR, final newline, long lines) |
| `PRESUBMIT.py` files | `fw depot presubmit python [--base REV] [--depot-tools DIR]` | runs the real `presubmit_support.py` under `uv` (the checks are Python `input_api` code, not ported) |
| `repo init\|list\|manifest` | `fw depot repo init -u URL -b REV`, `list`, `manifest` | equal on a small manifest (5 group filters) and on `android-17.0.0_r1` (1084, 1087, 852, 1084, 30 projects) |
| `repo sync` | `fw depot repo sync [-g groups] [-j N] [--shallow\|--partial\|--metadata-only] [paths]` | same commits and tree hash on 4 Android 17 projects, copyfile and linkfile equal; 0.98 s vs 2.1 s |
| `source build/envsetup.sh; lunch` | `fw depot lunch PRODUCT[-RELEASE-VARIANT] [--top DIR]` | variables derived from the tree (top, product, release, variant, sequence number, device from `PRODUCT_DEVICE`, out dirs, PATH); the Soong dumpvars (JAVA_HOME, clang paths, `PRODUCT_*`) are listed as not computed |

Also: `fw depot deps list DEPS` (JSON of a DEPS file for a target), `fw depot model` (work/span model of a siso build), `fw depot skeleton` (writes every recursed DEPS file without a clone, so `revinfo` runs offline).

## Kept as upstream binaries

- `gn`, `siso` and `ninja`: pinned CIPD packages installed by `fw depot tools` or `sync`. gn is not rewritten.
- n2 (evmar/n2, b1fead52) is not adopted: no `-t` tools and no `-n`; 5% slower on a compile-bound abseil build (906 edges: ninja 16.1 s, n2 17.0 s); faster on a 30k-edge synthetic graph (5.7 s vs 8.4 s) but slower on a null build (0.16 s vs 0.12 s).
- Git stays the `git` executable: shallow clone of platform/build/soong 0.74 s with git against 6.8 s with gitoxide 0.61 (zlib: 0.34 s vs 0.65 s); gitoxide wins only on a small full-history clone (0.46 s vs 0.59 s).
- `vpython3` in a hook falls back to `python3` when it is not on `PATH`.

## Not ported

gclient: git submodule dependencies (`git_dependencies`), patch refs, `deps_os`, `flatten`, `pack`, `grep`, `diff`, the git cache (`cache_dir`) and removal of unversioned trees. repo: superproject, `upload`, `forall`, manifest servers, smart sync, the shared `.repo/project-objects` store (projects are plain clones). autoninja: build telemetry and log uploads. CIPD: symlink install mode and `.cipd` state (state lives in `.fw-depot/cipd.json`), authentication for non-public packages.

## Auth for git cl

`FW_GERRIT_TOKEN` (bearer), `FW_GERRIT_AUTH=gcloud` (access token of `gcloud auth print-access-token`), else the `~/.gitcookies` entry of the host, else anonymous reads. `--gerrit-server URL` overrides the host derived from the remote (`x.googlesource.com` is reviewed on `x-review.googlesource.com`).
