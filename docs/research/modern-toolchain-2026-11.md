# Modern toolchain at every step, measured 2026-11-21

Lot: `docs/agents/queue/modern-toolchain-every-step.md`. The measurements and the gates are in the repository:

| Command | What it does |
|---|---|
| `bun tools/modern-toolchain/check.ts ignore [--write]` | no tracked path is ignored or build output; `.dockerignore` equals the set generated from `steps.toml` and `.gitignore` |
| `check.ts legacy` | no removed legacy tool in CI, scripts or container files (only `steps.toml` and `check.ts` are exempt from the scan) |
| `check.ts userland [--local]` | every adopted CLI runs `--version` at its `min_version` on every node of `cluster/build.toml` (and on the PC with `--local`) |
| `check.ts git-nodes [--apply]` | node git settings and `git maintenance` registration of the long-lived checkouts |
| `check.ts ssh` | the PC's ssh policy for every node and alias of `build.toml`: route probe, ed25519, no forwarding, post-quantum key exchange |
| `check.ts container` | the Dockerfile builder tag equals the `rust-toolchain.toml` channel and every workspace member is copied |
| `check.ts publish` | packaged size of the published crates and of npm packages without `files` stays under `package_budget_bytes` (needs cargo: run on a node) |
| `tools/modern-toolchain/bench/` | `ssh-latency.ts`, `git-clone-modes.sh`, `commit-by-path.sh`, `cli-compress.sh`, `codec-parse/` (Rust harness), `bun-parse.ts`, `ci-runs.ts`, `ci-jobs.ts` |

Hosts: n1 (alias fr1; 12 vCPU Intel Haswell, git 2.55.0, OpenSSH 10.5p1) and n2 (alias prd; 8 vCPU AMD EPYC 9354P, git 2.56.1, OpenSSH 10.6p2,
zstd 1.5.7, brotli 1.2.1, xz 5.8.4, gzip 1.15, rustc 1.100.0, bun 1.4.5); the PC runs Windows OpenSSH 10.0p2 and git 2.56.0.
The nodes were shared and loaded during the run (n2 load 25 to 55 on 8 cores, n1 80 to 110 on 12 cores), so compression and parsing figures are thread CPU
time (best of 7, best of 3 for the archive stage) or hardware instruction counts; wall times are labelled with the load. Raw outputs of the run stay on n2 in
`/srv/build/bench/modern-toolchain/results`.

| Step | Current | Candidates | Numbers | Decision | State |
|---|---|---|---|---|---|
| 1 SSH | Windows OpenSSH 10.0p2; nodes on 10.5p1 and 10.6p2; `mlkem768x25519-sha256` first, ed25519 keys, `forwardagent no`, `identitiesonly yes` (`ssh -G`) | route-table probe instead of `ping -n 1 -w 1000` in the `Match exec` blocks; OpenSSH 10.4.0.0p1-Preview; Git-for-Windows ssh 10.6p1 (has ControlMaster) | before (old config, aliases fr1 and prd, 5 runs, all rc 0): fr1 1250 to 1757 ms, prd 941 to 1282 ms; after: fr1 545 to 762 ms, prd 406 to 488 ms. 10.4p1 against 10.0p2, 10 runs each: fr1 628 to 956 against 634 to 998 ms, prd 431 to 495 against 405 to 536 ms, no difference. Git ssh 10.6p1 with `ControlMaster=auto`: each attempt fails with `mux_client_request_session: read from master failed: Connection reset by peer` | probe replaced in `~/.ssh/config` (backup `config.bak-modern-toolchain`); client not upgraded (no gain); no ControlMaster on Windows; `check.ts ssh` and the `legacy` gate keep probe, key type, forwarding and key exchange | done; a Rust SSH client or persistent tunnel not measured |
| 2 Git and VCS | git 2.56.1 on n2; full clones | blobless, treeless, depth 1, scalar, commit-graph, `git maintenance`, fsmonitor, jj, LFS, Xet | n2 at load 18 to 40, fw at 82,931 to 82,966 commits and 16,779 to 17,339 files (each line of the script prints its own counts): full clone 104.1 s plus checkout 4.8 s, `.git` 587,768 KiB; blobless 9.0 s plus 21.5 s, 172,752 KiB; treeless 4.9 s plus 19.0 s, 133,812 KiB; depth 1 7.6 s plus 2.6 s, 78,764 KiB; `scalar clone` 15.9 s (checkout of the top level only), 89,848 KiB. `git blame README.md`: full 143 ms, blobless 10,850 ms, treeless 223,067 ms, scalar 7,343 ms. `rev-list --count HEAD`: full 1,117 ms, 179 ms after `commit-graph write --reachable --changed-paths`; blobless 136 ms and 173 ms; depth 1 9 ms. Commit by path (temporary index flow, PC, 16,632 files): 158 ms (read-tree 51, hash-object and update-index 46, write-tree 33, commit-tree 27). `git status` in a clean 16,632-file checkout: fsmonitor and untracked cache 43.2 ms, both off 80.7 ms; in the shared fw tree 45.9 ms against 60.0 ms. `jj st` 366.3 ms against `git status --short` 115.9 ms. 23 tracked blobs of 1 MB and more total 83,174,349 B | CI and one-shot gate checkouts `--depth=1`; agent working clones blobless, never treeless (blame 1,560 times slower than a full clone, and `commit-graph write --changed-paths` downloads every tree: 308 MB and more than 8 minutes before it was stopped); `scalar clone` for a subset of the tree, then `git sparse-checkout add`; `git maintenance` on every long-lived checkout; fsmonitor stays off in the shared fw tree (14 ms gain, a daemon per checkout); jj not adopted (status 3.2 times slower than git, no gain over the temporary-index flow); no LFS and no Xet (no `filter=lfs` in `.gitattributes`, the 23 files over 1 MB total 83 MB, git-lfs 3.8.0 is installed on the PC) | done: `check.ts git-nodes --apply` set `feature.manyFiles`, `fetch.writeCommitGraph` and `maintenance.strategy=incremental` on n1 and n2 and registered 19 checkouts on n1 and 1 on n2; the `git-maintenance@hourly`, `daily` and `weekly` timers are active on both nodes. Sparse cones of the kernel and Zed trees not re-measured |
| 3 Ignore files | 30 ignore files; hand-written `.dockerignore` of another lane | one generated set; `.npmignore`; cargo `exclude` | `.dockerignore` is generated (22 lines of `steps.toml [dockerignore]` plus every `.gitignore` pattern: `**/` added to unanchored patterns, trailing slash dropped, negation kept); drift fails `check.ts ignore`. The 15 published crates (closure of `product.toml crates_io_publish_order`) package 3,165 to 264,615 B by `cargo package --list`: no `exclude` needed. npm packages without `files`: `ay-wck` tracks 9,365,631 B and `ay-env-kit` 1,061,989 B | `files` added to both packages; `package_budget_bytes` 1,048,576 gates the others (`check.ts publish`) | done |
| 4 Compression | `tar -czf` for the Linux release assets (`aphrody-release.yml`); brotli and zstd on the CDN | zstd, brotli, xz; crates flate2 (zlib-rs), miniz_oxide, libdeflater, zstd, ruzstd, brotli, liblzma, lzma-rs, tar, zip | ELF `fw` 5,677,752 B: gzip -9 2,556,359; xz -9 1,855,380; zstd -19 1,983,349; brotli q11 1,885,193. Wasm 6,113,607 B: gzip 2,644,168; xz 1,863,704; zstd -19 2,063,593; brotli q11 1,947,642. Tar of `crates/` 15,493,120 B: gzip 3,667,004; xz 2,422,192; zstd -19 2,489,705; brotli q11 2,398,564; zstd -3 3,413,895 in 100 ms CPU. zstd -19 is 2.8 to 10.7 percent above xz and decompresses 2.7 to 3.1 times faster (24, 34 and 43 ms against 74, 93 and 117 ms); brotli q11 is 3.7 to 5.6 percent below zstd -19 at 6 times the compression CPU | Linux archives `tar.zst` level 19; the CDN keeps serving zstd (already live on all four URLs checked); brotli q11 only for artifacts that must be pre-compressed; crates: `zstd` for archives, flate2 with zlib-rs where gzip is mandatory, `zip` only where a zip is mandatory; lzma-rs rejected (it stores), ruzstd for decoding only | measured; the release workflow belongs to the release train: switch returned to the coordinator |
| 5 Parsing | serde_json 1.0.154, toml 1.2.2, pulldown-cmark 0.13.6, csv 1.4.0, syn 2 | sonic-rs 0.5.12, simd-json 0.19.0, serde_norway 0.9.42, serde-saphyr 2.2.0, saphyr 0.4.0, yaml-rust2 0.13.2, tree-sitter 0.27.2, comrak 1.1.0, Bun built-ins | JSONL of 7,443,266 B, thread CPU: serde_json 6.6 ms, sonic-rs 2.8 ms, simd-json 9.0 ms. YAML (106 files): yaml-rust2 4.8 ms, saphyr 5.1 ms, serde_norway 6.4 ms, serde-saphyr 11.5 ms; `Bun.YAML.parse` 2.5 ms. TSV and CSV (65 files): csv StringRecord 4.2 ms, ByteRecord 5.5 ms, `str::split` 2.4 ms. TOML (446 files): `toml::Table` 35.5 ms; `Bun.TOML.parse` 17.7 ms (444 files). Rust source (5,217 files, 133 MB): syn 11.3 s, tree-sitter-rust 16.1 s. Markdown (2,090 files): pulldown-cmark 100.3 ms, comrak 170.3 ms | sonic-rs for hot JSON loads of 1 MB and more; simd-json rejected; yaml-rust2 or saphyr where Rust parses YAML; keep csv, toml, pulldown-cmark; in Bun code use `Bun.YAML`, `Bun.TOML`, `JSON.parse` | measured; applying sonic-rs belongs to the crates lane |
| 5b Duplicate parsers | `Cargo.lock` at origin/main | one version each | toml 0.5.11, 0.8.23, 0.9.12 (about 20 aphrody crates) and 1.2.2; zstd 0.13.5 (Zed crates) and 0.14.4; zip 8.6.0 and 9.8.1; brotli 7.0.0 and 9.1.3; pulldown-cmark 0.10.3 (mdbook) and 0.13.6 | align the aphrody crates on the workspace toml and zstd | returned to the crates lane |
| 6 Build tooling | mold and wild linkers, sccache, cargo-nextest on n1 | decided by the crate lot | a cold build of the harness (178 packages including zstd-sys, brotli, tree-sitter, liblzma) took 9 min 19 s on n2 at load 33 to 55; nextest against `cargo test` stays unmeasured (a cold nextest build was stopped after 25 minutes at load 23 to 47) | none | open: needs a warm target on an idle node |
| 7 CI/CD | 24 workflow files; `CI` runs on every push to main | concurrency, path filters, batching | `ci.yml` over the 7-day window to 2026-11-21T10:32Z (it exists since 2026-11-19T07:22Z): 420 runs, 330 cancelled, 83 failure, 5 success, 2 pending or running; the latest 51: 50 cancelled, 1 pending. main took 626 commits in 24 hours (one per 2.3 minutes) against a 643 s median for `Rust (linux)` (3 completed runs, 482 to 723 s, all failing). Jobs of the 3 latest completed runs: Retired surfaces 16 to 19 s, Layout map 15 to 16 s, Hardcoded 14 to 18 s, Python and purity 13 to 21 s, Rust hygiene 210 to 260 s, crates.io packaging 101 to 237 s, Wasm and musl 268 to 281 s, Bun and edge 178 to 397 s, Kotlin 137 to 170 s. `ci-windows.yml`: 109 runs, 102 cancelled, 6 failure, 1 running; its Windows job took 212 to 246 s (5 runs) after leaving `ci.yml`, where it took 2,506 s | the three pure-Bun gates (retired surfaces, hardcoded, layout) move to `ci-gates.yml` with a per-commit concurrency group, so every push gets a verdict in 15 to 20 s instead of being cancelled while pending; `ci.yml` keeps the compiling jobs; this lot's gate step runs with `if: ${{ !cancelled() }}` and adds `check.ts container`. Heavy jobs on main cannot finish at one push per 2.3 minutes: batching (a scheduled run on the latest main plus `workflow_dispatch`) is returned as a new lot | partly done |
| 8 Containers | `Dockerfile`: `rust:1.100-alpine` builder, `alpine:3.22` runtime; podman 5.8.6 and buildah 1.43.2 on n1 | build inside the image with the whole workspace; distroless; SBOM and provenance | `podman build` on n1 (load 83 to 109): the Dockerfile of origin/main fails after 124 s with `failed to load manifest for workspace member /src/aphrody/crates/*` (exit 101); the first fix, `COPY . .`, fails after 207 s with `git is not available` for the aphrody-labs/accesskit git dependency; the second fix (git installed) is building on n1 and on n2, result in the next update of this row | the Dockerfile copies the workspace (`COPY . .` with the generated `.dockerignore`), installs git, mounts the cargo caches; `check.ts container` fails the previous file | open: the image build of the fixed file has no finished result yet |
| 9 Browser product | `aphrody.com` and `dragonballfr.com` | Lighthouse 13.6.0, AVIF, font subsetting | Lighthouse 13.6.0, default mobile profile, one run each. aphrody.com: performance 100, accessibility 100, best practices 100, SEO 100; FCP 0.9 s, LCP 0.9 s, TBT 60 ms, CLS 0; 7 requests, 14 KiB (`tokens.css` 27,592 B raw, 4,459 B transferred). dragonballfr.com: performance 40, accessibility 100, best practices 96, SEO 100; FCP 2.5 s, LCP 12.0 s, TBT 1,250 ms, 122 requests, 1,592,395 B transferred (fonts 558,635, scripts 575,314, images 204,516, stylesheets 136,131) | none for aphrody.com; dragonballfr.com goes to a new lot | measured |
| 10 Agent CLIs | rg, jaq, fd, zstd, brotli, uutils on the nodes | rg, fd, jaq, sd, xh, uutils, in-process fetch | n2, hyperfine 2 median CPU ms and wall ms (instruction counts in the harness output): `grep -rIl` 295 and 353 against `rg -l` 369 and 265 (no match), 409 and 475 against 376 and 290 (many hits); `find -name '*.toml'` 92 and 120 against `fd -e toml` 130 and 110; `jq -c .` on the 7.4 MB JSONL 135 and 169 ms (1,848,690,956 instructions) against `jaq -c .` 53 and 69 ms (405,778,527); `sed` 5 and 9 ms against `sd` 8.4 and 17.6 ms on `Cargo.lock`. PC to `api.github.com/repos/aphrody-labs/fw`: `curl` 167 ms, `xh` 218 ms, `gh api` 579 ms; raw file: `curl` 109 ms, `xh` 149 ms; Bun `fetch` 315 ms first, then 19 to 25 ms on the kept-alive connection. n1 and n2 already link sort, cat and wc to uutils coreutils 0.12.0 | adopt rg, jaq, fd, zstd, brotli, coreutils (uutils) on every node and the PC; sd and xh rejected; API loops run in one Bun process; `check.ts userland` runs `--version` against a `min_version` | done on the nodes and the PC (zstd 1.5.7 installed on the PC) |

## Detail

### Step 1, SSH
`bun tools/modern-toolchain/bench/ssh-latency.ts [--config <file>] [--hosts a,b]` times `ssh <host> true` and reads the remote load in the same session. A first "before" attempt
with the names n1 and n2 against the old backup config measured failures: that config does not know those names and each call failed in 2.7 s (return code 255). The valid pair above uses
fr1 and prd, which the old config knows. The remaining floor is the network round trip plus key exchange and authentication: about 450 ms to n2 at load 32, about 700 ms to n1 at load 99.

### Step 2, Git
`bash tools/modern-toolchain/bench/git-clone-modes.sh <url> <scratch> [blame-path]` clones one mode after another and runs the commands an agent runs on the clone; `commit-by-path.sh <repo> <path>` times the temporary-index commit flow without touching refs. The full and blobless lines come from one run, treeless and depth 1 from a second (the first run was stopped inside the treeless commit-graph step, which the script now skips), scalar from a third, so the commit counts differ by a few commits pushed meanwhile. n2 load was 37 to 40, 18 to 20 and 24 respectively.

```
{"mode":"full","clone_ms":104092,"checkout_ms":4846,"git_kib":587768,"files":16779,"commits":82931,"rev_list_ms":1117,"rev_list_commit_graph_ms":179,"log_since_ms":44,"blame_ms":143,"load":"40.39 39.74 34.62"}
{"mode":"blobless","clone_ms":9028,"checkout_ms":21521,"git_kib":172752,"files":16813,"commits":82937,"rev_list_ms":136,"rev_list_commit_graph_ms":173,"log_since_ms":44,"blame_ms":10850,"load":"37.46 38.63 34.53"}
{"mode":"treeless","clone_ms":4886,"checkout_ms":19036,"git_kib":133812,"files":17339,"commits":82958,"rev_list_ms":232,"rev_list_commit_graph_ms":null,"log_since_ms":81,"blame_ms":223067,"load":"20.04 20.64 26.14"}
{"mode":"depth1","clone_ms":7635,"checkout_ms":2561,"git_kib":78764,"files":17339,"commits":1,"rev_list_ms":9,"rev_list_commit_graph_ms":8,"log_since_ms":10,"blame_ms":14,"load":"18.27 20.21 25.91"}
{"mode":"scalar","clone_ms":15859,"checkout_ms":0,"git_kib":89848,"files":17339,"commits":82966,"rev_list_ms":998,"rev_list_commit_graph_ms":82,"log_since_ms":41,"blame_ms":7343,"load":"23.96 20.60 24.63"}
```

### Step 4, compression
Inputs: ELF `fw` release build 5,677,752 B (`target/bench-r2m/release/fw` on n2), wasm `ay/packages/auto-native/wasm/auto_ffi_bg.wasm` 6,113,607 B, tar of `crates/` 15,493,120 B (1,189 tracked files).
`cli-compress.sh` gives CLI sizes and CPU ms; `codec-parse compress` and `codec-parse archive` give crate sizes and thread CPU ms.

| Input | Codec | Size B | Compress CPU ms | Decompress CPU ms |
|---|---|---|---|---|
| fw-release-elf (5,677,752 B) | gzip -9 | 2,556,359 | 674 | 27 |
| fw-release-elf (5,677,752 B) | xz -9 | 1,855,380 | 3,027 | 74 |
| fw-release-elf (5,677,752 B) | zstd -3 | 2,379,086 | 49 | 15 |
| fw-release-elf (5,677,752 B) | zstd -19 | 1,983,349 | 2,684 | 24 |
| fw-release-elf (5,677,752 B) | zstd -19 --long=27 | 1,983,287 | 2,918 | 27 |
| fw-release-elf (5,677,752 B) | brotli -q 11 | 1,885,193 | 16,527 | 39 |
| fw-release-elf (5,677,752 B) | brotli -q 5 | 2,162,283 | 285 | 33 |
| auto_ffi_bg.wasm (6,113,607 B) | gzip -9 | 2,644,168 | 560 | 32 |
| auto_ffi_bg.wasm (6,113,607 B) | xz -9 | 1,863,704 | 3,500 | 89 |
| auto_ffi_bg.wasm (6,113,607 B) | zstd -3 | 2,508,467 | 50 | 23 |
| auto_ffi_bg.wasm (6,113,607 B) | zstd -19 | 2,063,593 | 3,088 | 34 |
| auto_ffi_bg.wasm (6,113,607 B) | zstd -19 --long=27 | 2,063,533 | 3,267 | 28 |
| auto_ffi_bg.wasm (6,113,607 B) | brotli -q 11 | 1,947,642 | 17,907 | 36 |
| auto_ffi_bg.wasm (6,113,607 B) | brotli -q 5 | 2,277,660 | 356 | 37 |
| crates.tar (15,493,120 B) | gzip -9 | 3,667,004 | 1,204 | 46 |
| crates.tar (15,493,120 B) | xz -9 | 2,422,192 | 8,632 | 117 |
| crates.tar (15,493,120 B) | zstd -3 | 3,413,895 | 100 | 30 |
| crates.tar (15,493,120 B) | zstd -19 | 2,489,705 | 8,240 | 43 |
| crates.tar (15,493,120 B) | zstd -19 --long=27 | 2,473,223 | 9,216 | 47 |
| crates.tar (15,493,120 B) | brotli -q 11 | 2,398,564 | 29,439 | 57 |
| crates.tar (15,493,120 B) | brotli -q 5 | 3,025,330 | 784 | 48 |

Crates, `codec-parse compress` on the ELF, then on the wasm, then `codec-parse archive` on `crates/` (3 runs):

```
-- compress 5677752 B
flate2/zlib-rs 9                2548722 B  ratio 0.4489  compress     282.4 ms  decompress     19.5 ms  roundtrip-ok
miniz_oxide 9 (zlib)            2560499 B  ratio 0.4510  compress     555.0 ms  decompress     16.4 ms  roundtrip-ok
libdeflater 12 (gzip)           2479712 B  ratio 0.4367  compress     668.7 ms  decompress      9.1 ms  roundtrip-ok
zstd crate 3                    2376974 B  ratio 0.4186  compress      27.2 ms  decompress      6.9 ms  roundtrip-ok
zstd crate 19                   1982600 B  ratio 0.3492  compress    2383.1 ms  decompress     10.1 ms  roundtrip-ok
ruzstd fastest encoder          3321006 B  ratio 0.5849  (encoder ran)
ruzstd decoder (pure Rust)   decompress     24.8 ms of a zstd -19 stream  roundtrip-ok
brotli crate q5                 2162166 B  ratio 0.3808  compress     252.4 ms  decompress     21.2 ms  roundtrip-ok
brotli crate q11                1888351 B  ratio 0.3326  compress   11739.0 ms  decompress     24.5 ms  roundtrip-ok
liblzma xz 9                    1855380 B  ratio 0.3268  compress    2919.0 ms  decompress     74.8 ms  roundtrip-ok
lzma-rs (pure Rust xz)          5678068 B  ratio 1.0001  compress       1.0 ms  decompress      1.9 ms  roundtrip-ok
```

```
-- compress 6113607 B
flate2/zlib-rs 9                2641213 B  ratio 0.4320  compress     320.8 ms  decompress     22.5 ms  roundtrip-ok
miniz_oxide 9 (zlib)            2645922 B  ratio 0.4328  compress     527.7 ms  decompress     18.7 ms  roundtrip-ok
libdeflater 12 (gzip)           2511086 B  ratio 0.4107  compress     818.3 ms  decompress     11.2 ms  roundtrip-ok
zstd crate 3                    2506193 B  ratio 0.4099  compress      40.6 ms  decompress     10.3 ms  roundtrip-ok
zstd crate 19                   2063299 B  ratio 0.3375  compress    3070.6 ms  decompress     12.5 ms  roundtrip-ok
ruzstd fastest encoder          3627126 B  ratio 0.5933  (encoder ran)
ruzstd decoder (pure Rust)   decompress     35.5 ms of a zstd -19 stream  roundtrip-ok
brotli crate q5                 2277931 B  ratio 0.3726  compress     411.2 ms  decompress     21.0 ms  roundtrip-ok
brotli crate q11                1950491 B  ratio 0.3190  compress   13818.8 ms  decompress     29.1 ms  roundtrip-ok
liblzma xz 9                    1863704 B  ratio 0.3048  compress    3568.7 ms  decompress     79.2 ms  roundtrip-ok
lzma-rs (pure Rust xz)          6113944 B  ratio 1.0001  compress       0.8 ms  decompress      1.6 ms  roundtrip-ok
```

```
-- archive of crates/: 1189 files, tar 15352320 B
tar crate + zstd crate 19       2483226 B  ratio 0.1617  compress    8360.2 ms  decompress     20.5 ms  roundtrip-ok
zip crate deflate 9             4236206 B  ratio 0.2939  compress     380.5 ms  (zip of 1189 files, 14415001 B of content)
zip crate zstd 19               3894804 B  ratio 0.2702  compress   89323.7 ms  (zip of 1189 files, 14415001 B of content)
```

`lzma-rs` stores the data (ratio 1.0001), so it is no xz replacement; `ruzstd`'s encoder gives ratio 0.58 to 0.59 against 0.42 for `zstd` level 3 and its decoder is 2 to 3 times slower than the C library; `zip` compresses file by file (4,236,206 B for deflate 9 and 3,894,804 B for zstd 19 against 2,483,226 B for one tar compressed once) and its zstd method needed 89.3 s CPU. For `tar.zst` of many files the `tar` crate plus the `zstd` crate is the shape to use.

### Step 5, parsing
Corpus: tracked files of the repository: JSONL `plugins/aphrody/skills/bun-reference/data/bun.jsonl` (11,543 lines, 7,443,266 B), 106 YAML files, 65 TSV and CSV files, 446 manifests, 5,217 Rust files (132,957,458 B), 2,090 Markdown files.
`codec-parse` is a standalone harness with its own empty workspace and lockfile; `bun-parse.ts` runs the Bun built-ins on the same files.

```
-- JSONL 11543 lines 7443266 B (/srv/build/bench/modern-toolchain/fw/plugins/aphrody/skills/bun-reference/data/bun.jsonl)
serde_json::Value                        6.6 ms    1127.3 MB/s  parsed 11543/11543
sonic_rs::Value                          2.8 ms    2681.1 MB/s  parsed 11543/11543
simd_json owned (incl. copy)             9.0 ms     825.8 MB/s  parsed 11543/11543
-- YAML 106 files 277230 B
serde_norway -> Value                    6.4 ms      43.1 MB/s  parsed 106/106
serde-saphyr -> Value                   11.5 ms      24.1 MB/s  parsed 106/106
yaml-rust2 YamlLoader                    4.8 ms      57.3 MB/s  parsed 106/106
saphyr Yaml                              5.1 ms      53.9 MB/s  parsed 106/106
-- CSV/TSV 65 files 2173540 B
csv ByteRecord                           5.5 ms     397.7 MB/s  parsed 9880/9880
csv StringRecord                         4.2 ms     513.8 MB/s  parsed 9880/9880
str::split (no quoting)                  2.4 ms     914.1 MB/s  parsed 9909/9909
-- TOML 446 files 2511252 B
toml::Table                             35.5 ms      70.8 MB/s  parsed 446/446
-- Rust source 5217 files 132957458 B
syn::parse_file                      11287.7 ms      11.8 MB/s  parsed 5214/5217
tree-sitter-rust (no errors)         16071.0 ms       8.3 MB/s  parsed 5196/5217
-- Markdown 2090 files 20321815 B
pulldown-cmark html                    100.3 ms     202.7 MB/s  parsed 23440674/23440674
comrak html                            170.3 ms     119.3 MB/s  parsed 23753486/23753486
```

Bun built-ins on the same files (`bun-parse.ts`, best of 7): `Bun.YAML.parse` 2.5 ms for the 106 YAML files (108 MB/s), `Bun.TOML.parse` 17.7 ms for the 444 manifests it accepts (the Rust `toml::Table` takes 35.5 ms for 446), `JSON.parse` per line 5.1 ms and on the whole array 4.9 ms for the JSONL (serde_json 6.6 ms, sonic-rs 2.8 ms).

### Step 7, CI/CD
`bun tools/modern-toolchain/bench/ci-runs.ts [--days 7 | --latest 51] [--workflow ci.yml]` tallies runs by conclusion from the live API; `ci-jobs.ts [--runs N]` gives the duration of each job of the latest successful or failed runs. The previous version of this document gave 51 runs in 7 days, 44 of them cancelled and none successful: that was wrong (the window held 326 runs at the time, 5 of them successful); the figures above replace it. The workflow-level group keeps one pending run and cancels the previous pending one on every push, even with `cancel-in-progress: false` for main, which is why runs end cancelled before any job starts. The `Retired surfaces` job was red for 4 of 4 completed runs; one of its 20 violations was a line of the previous version of this document that quoted a retired pattern, now reworded.

### Step 8, containers
The Dockerfile of origin/main copied only `crates`, `xtask` and `core/webgpu`, while the root workspace lists members under `aphrody/crates/*` and `ay/crates/*` and cargo loads every member manifest: the image could not build since the monorepo import, and the CI job named `Wasm32 + Linux musl + Alpine OCI` only runs `cargo check`, so nothing caught it. `check.ts container` now fails on that shape (it reports the missing `aphrody/` and `ay/` copies on the old file) and on a builder tag that differs from the `rust-toolchain.toml` channel.

### Step 9, browser product
`curl -D - -H 'Accept-Encoding: zstd, br, gzip'` on `aphrody.com/`, `aphrody.com/tokens.css`, `dragonballfr.com/` and a `cdn.dragonballfr.com` stylesheet (664,798 B raw) answers `Content-Encoding: zstd` and `alt-svc: h3` on all four. Cloudflare re-encodes dynamically, so pre-compressed brotli q11 (3.7 to 5.6 percent smaller than zstd -19 in step 4) has no effect on these hosts. aphrody.com is at the ceiling of the four categories: no AVIF or font subsetting to win. dragonballfr.com is not: LCP 12.0 s with 558,635 B of font transfer (Noto Sans JP chunks) and a 664,798 B stylesheet chunk are the lever; it belongs to the shenron lane and is returned as a new lot.

### Step 10, agent CLIs
`cli-loops.sh <tree> <jsonl> <text>` runs the pairs with hyperfine 2 (hardware counters); `check.ts userland --local` printed on 2026-11-21: rg 15.2.0, jaq 3.1.2 (n2 3.2.0), fd 10.5.2 (n2 10.5.0), zstd 1.5.7, brotli 1.2.1 (n1 1.2.0), coreutils 0.12.0 on the PC, n1 and n2. rg wins on wall time through threads while using about as much CPU as grep; fd is at parity with find on wall time (no speed gain: it stays for its ignore-aware defaults); jaq is 2.5 times cheaper than jq; sd is slower than sed on the common small edit and is rejected; `xh` is slower than `curl` and `gh api` costs 579 ms per call, so loops of API calls run in one Bun process or one `gh api --paginate`.
