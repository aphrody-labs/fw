# Rust toolchain upstreams, audit of 2026-11-21

Scope: rust-lang/rust, rust-lang/cargo, rust-lang/rustup, cross-rs/cross, rust-cross/cargo-xwin, Jake-Shadle/xwin, rust-cross/cargo-zigbuild, llvm/llvm-project, DioxusLabs/rustd.
Every value below was read from the GitHub API (`gh api`, run 2026-11-21 between 08:55 and 09:20 UTC) or from a command on the cluster nodes in the same session. The factory built from this audit is `fw toolchain` (crates/fw-toolchain, declared in cluster/toolchain.toml, resolved into cluster/toolchain.lock.toml).

## State of each upstream

| repository | default branch head | open PRs | PRs merged since 2026-10-22 | head CI (check runs) | newest release | open issues |
| --- | --- | ---: | ---: | --- | --- | ---: |
| rust-lang/rust | main, 2026-11-20 | 1462 | 866 | success 30 | 1.100.0 (2026-11-12) | 12599 |
| rust-lang/cargo | master, 2026-11-21 | 96 | 100 | success 30 | follows rustc | 1708 |
| rust-lang/rustup | main, 2026-11-20 | 30 | 48 | success 24, skipped 6 | 1.29.3 (tag commit 2026-11-02) | 428 |
| cross-rs/cross | main, 2026-10-31 | 33 | 4 | failure 5, success 23 | v0.2.5 (2023-02-04) | 259 |
| rust-cross/cargo-xwin | main, 2026-11-04 | 1 | 8 | success 26 | v0.23.1 (2026-08-13) | 1 |
| Jake-Shadle/xwin | main, 2026-08-12 | 11 | 0 | success 7 | 0.10.0 (2026-08-12) | 33 |
| rust-cross/cargo-zigbuild | main, 2026-11-16 | 3 | 11 | success 29 | v0.24.1 (2026-11-04) | 9 |
| llvm/llvm-project | main, 2026-11-21 | 11946 | 6135 | skipped 30 | llvmorg-23.1.6 (2026-11-17) | 42924 |
| DioxusLabs/rustd | main, 2026-10-21 | 8 | 0 | failure 12 (11 are `fuzz`), success 18 | nightly (2026-10-21, pre-release) | 21 |

Reading of the table, per project:

- rustc: stable is 1.100.0 (tag commit 0ef1bc386059596dada8fb65b50425aab2ea637a, 2026-11-09), beta is 1.101.0, main is 1.102.0. 1.99.1 was a point release on 2026-10-16. `rustc -vV` of the 1.100.0 toolchain on fr1 reports `LLVM version: 23.1.5`. fw's rust-toolchain.toml already says 1.100.0.
- rustc's LLVM: the gitlink `src/llvm-project` at tag 1.100.0 is rust-lang/llvm-project commit 0a5579eb649d3634a9c30ac873300cc2944fa070, from branch `rustc/23.1-2026-07-22` (.gitmodules). The tip of that branch moved on to 08ec928a5bd1866f4008c8ad216166ca685fd380 (main's pin, 2026-10-29). LLVM 23.1.5 exists as an upstream release (llvmorg-23.1.5, 2026-11-03) with prebuilt Linux X64 and ARM64 tarballs and sha256 digests; 23.1.6 is newer (2026-11-17) but not what rustc expects.
- cargo: unchanged cadence, built inside rust-lang/rust releases; no separate pin needed.
- rustup: healthy, 1.29.3; the factory does not use it to install (it fetches the dist manifest directly and checks hashes), so rustup is only a client of the same dist server.
- cross-rs/cross: last release 2023-02-04; main still gets small fixes (4 merges in 30 days), CI on main has 5 failing check runs. It runs targets in docker or podman containers; fr1 has no docker (`docker: command not found`), so it is not usable on the nodes. Kept as a fork, marked `used = false` in cluster/toolchain.toml.
- cargo-xwin: active (v0.23.1; 8 merges since 2026-10-22), 1 open issue. It wraps xwin and clang-cl style flags for `*-pc-windows-msvc`.
- xwin: the library cargo-xwin sits on has no maintainer: issue #193 "Looking for Maintainer" (2026-11-16), last merge 2026-08-12. Two open issues matter to the factory: #194, VSIX unpack path traversal (a member named `Contents/lib/../../...` is written outside the unpack directory), and #192, the license prompt links the 2019 build tools terms for manifest 17 and 18. Both are fixed in our fork (see "Changes in forks").
- cargo-zigbuild: active (11 merges in 30 days), v0.24.1; links with `zig cc`, so one zig binary gives glibc-versioned gnu and musl targets for x86_64 and aarch64. fr1 has zig 0.17.1 (`zig version`).
- llvm/llvm-project: only the version rustc expects matters; main is 24-init, the release branch is 23.1.x.
- rustd: a prototype (README: "Nobody outside the project has used it"). Pinned to one toolchain, Rust 1.99.0 at publication; builds programs only for ELF Linux and macOS arm64; `cargo rustd` has not been run on Windows although the daemon has (harness). Last commit 2026-10-21; 21 open issues, among them #27 (a first build still loading after 19 min at 41 GB RSS) and #28. The binaries on the `nightly` release are Cranelift only (aarch64-apple-darwin, aarch64 and x86_64 Linux, x86_64 Windows).

## Decisions

1. Rust toolchain: install rustc, cargo, rust-std and rust-src straight from the dist manifest of the channel in rust-toolchain.toml. The manifest gives a sha256 per artifact; `fw toolchain pin` freezes them in the lock and `fw toolchain fetch` refuses a mismatch, then checks that the installed `rustc -vV` reports the pinned commit.
2. LLVM: take the prebuilt LLVM release whose X.Y.Z equals the version rustc reports (derived from the LLVM version file at the gitlink commit, not typed in). Building LLVM from rust-lang/llvm-project stays possible because the fork carries the pinned commit.
3. Cross targets: msvc through xwin + cargo-xwin, gnu and musl (x86_64 and aarch64) through cargo-zigbuild, wasm32 through plain cargo. cross-rs/cross is not a build path on these nodes.
4. Forks (aphrody-labs): rust, cargo, rustd already existed; rustup, cross, cargo-xwin, xwin, cargo-zigbuild and llvm-project were created on 2026-11-21. `forks-follow.yml` (hourly merge-upstream of every non-archived fork's default branch) keeps them current; llvm-project's default branch is the upstream `landing-page`, so the rustc branch and the pinned commit are plain refs in the fork, created through the API.
5. rustd: port to the pinned toolchain instead of keeping 1.99.0, see the rustd section.

## Changes in forks

- aphrody-labs/xwin: refuse any VSIX member whose path after `lib` or `include` has a non-plain component (closes the #194 class), and print the terms URL of the manifest version (17: VS 2022, 18: VS 2026) instead of the 2019 fwlink (#192).
- aphrody-labs/llvm-project: ref `rustc/23.1-2026-07-22` created at the tip of the upstream branch.
- aphrody-labs/rustd: all upstream branches mirrored (the submodule `vendor/rust-analyzer` points at a commit of the `rust-analyzer` branch, which the fork needs to be able to check out).


## rustd on the pinned toolchain (measured 2026-11-21)

Fork aphrody-labs/rustd, `main` at 6f7c02192220f6af504d73dbbc0b648dd37b3021 (three commits on DioxusLabs/rustd ebcd234a02f7): 48820d0 moves the pin to the channel of rust-toolchain.toml (`harness/toolchain`, CI, install scripts, README), takes `vendor/rustc_abi` from the 1.100.0 tag (it adds `Float::F16B`) and the submodule commit e750978b2805703c78f7757d90637377ed1f742d on branch `rust-analyzer-aphrody` (one arm: `F16B` lowers to the error type, Rust has no bf16 name); 0fd1b39 and 6f7c021 are the `cargo rustd` changes below. Records of what was measured on 1.99.0 (results, baselines, platform notes) are left as written.

Defects found by running it, fixed in the fork:

1. The daemon looked up the records of a build with the first ancestor named `build`, `deps` or `examples`. A workspace below a directory called `build` (the cluster node path `/srv/build/...`) found no record and every first build failed with "cargo built `x` before rustd recorded what it is linked from". The profile directory is now the one that holds `.cargo-lock`.
2. File names (`.exe`, `.dll`, `.lib`, `lib*.so`) came from the host's constants; they now follow the unit's `--target`.
3. `cargo rustd` refused every COFF target. With xwin's `LIB` and `-Lnative` flags in the environment (`eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"`) it now builds `x86_64-pc-windows-msvc` from Linux. rustc runs `lld-link -flavor link`; the daemon moved all arguments into a response file, where `-flavor link` is read as a file name, so the flavor pair stays on the command line.

Proof (fr1, `winsmoke`: `fn value() -> u64`, rust-toolchain 1.100.0, xwin SDK 10.0.26100): first build 63.1 s (cargo 0.48 s, then the daemon loads the workspace and links), file type `PE32+ executable for MS Windows 6.00 (console), x86-64`; run under `wine64` it prints `answer 21 on windows`; after editing the literal, rebuild 0.74 s then 0.42 s (`Finished dev profile [rustd] in 0.24s` and 0.09s), output `answer 34 on windows`.

Edit-rebuild on fw-bin (prd, commit f5627a92a2530f7201c0f1845820c6019c05c008, dev profile, `from_millis` literal in `crates/fw-bin/src/doctor.rs`, no sccache, host load average 38 to 50 from other jobs, Rust 1.100.0, 8 cores):

| step | plain cargo | `cargo rustd` |
|---|---:|---:|
| cold build of fw-bin | 125.6 s | 251.8 s (cargo builds the dependencies, then the daemon loads and links) |
| edit, 8 runs, seconds | 5.56 4.56 4.11 4.86 6.61 7.91 4.47 6.52 (median 5.21) | 1.43 1.78 1.76 0.98 1.12 1.11 1.81 0.83 (median 1.28) |

Median speedup of the edit rebuild is 4.1x on this workload at this load, not 25x. The edited program was checked after two edits (sha256 of `target-rustd/debug/fw` changed and `fw --version` ran). Upstream's own table (docs/cargo.md) shows 1 to 2 s for ripgrep edits too; the large ratios in its harness come from the LLVM rows and from projects with slower rustc builds. Not run: the python harness gate (`difftest.py`, `programs.py`, `splices.py`, ...), a Windows host, `cargo rustd test` (refused upstream). Run: `cargo test --release -p rustd -p hir-ty` passes (47 and 107 tests, 0 failed).
