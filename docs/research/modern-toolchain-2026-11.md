# Modern toolchain at every step (2026-11-21)

Lot of `docs/agents/queue/modern-toolchain-every-step.md`. One row per step: current, candidates, numbers, decision. Every number below was produced by a command run on 2026-11-21 on the nodes (prd: AMD EPYC 9354P, 8 cores, build slice; fr1: 12 cores) or on the PC. Raw benchmark scripts are not kept in the repository; the commands are named in each row.

| Step | Current | Candidates measured | Numbers | Decision |
|---|---|---|---|---|
| 1 SSH | Windows OpenSSH 10.0p2; the PC config probed the mesh with `Match ... exec "ping -n 1 -w 1000 <mesh address>"` before every connection | ControlMaster (absent on Windows OpenSSH), probe by route table | `ssh fr1 true` 1366, 1536, 1421 ms with the ping probe (mesh down), 686, 578, 588 ms with a `route print ... findstr` probe; kex negotiated is `mlkem768x25519-sha256`, auth is ed25519, agent forwarding off | Probe by route table (PC-local `~/.ssh/config`, not tracked). Key types and kex are already modern. A persistent multiplexed client stays open (see remaining) |
| 2 git and VCS | git 2.56.1 (prd), 2.55.0 (fr1), 2.56.0 (PC) | full, blobless, treeless, depth 1 clone; commit-graph; sparse cone; jj 0.46.0 | clone of aphrody-labs/fw (82,820 commits, 16,614 files): full 40.8 s / 586 MB, blobless 22.5 s / 172 MB, treeless 22.8 s / 133 MB, depth 1 11.4 s / 78 MB; `git blame README.md`: full 103 ms, blobless 9,210 ms, treeless 160,664 ms; `rev-list --count HEAD` 678 ms without commit-graph, 74 ms with it; sparse cone `crates tools docs` leaves 1,549 of 16,614 files; PC `git status` 98 ms vs `jj st` 170 ms, `jj commit README.md` 262 ms vs `git add` 55 ms + `git commit` 362 ms | CI keeps `actions/checkout` depth 1 (cheapest clone). Persistent checkouts get blobless at most; treeless is rejected (blame 160 s). Commit-graph written with `fetch.writeCommitGraph` and `feature.manyFiles` on fr1 (global config) and a one-time `commit-graph write --reachable --changed-paths` on its 5 full checkouts (`/home/ubuntu/src/bun`: rev-list 1,169 ms without, 87 ms with). jj gives no gain over the git plumbing flow of the agents: not adopted |
| 3 ignore files | 30 tracked ignore files, no root `.dockerignore` | `git ls-files -ci --exclude-standard` on a fresh clone | 6 tracked paths matched an ignore rule: 3 `ay/crates/auto-sdk/tests/data/*` (`*.p12`, `*.pem`) and 3 `crates/fw-browser-engine/build/engines/*` (`build/`); 1 tracked executable (`aphrody/crates/zed/resources/windows/bin/x64/OpenConsole.exe`, a vendored resource) | Negations added in `ay/.gitignore` and `.gitignore`; root `.dockerignore` added (no `.git`, `target`, `node_modules`, caches, keys). Verified in a throwaway repo that the six paths are no longer ignored and `target/` and other `*.pem` still are |
| 4 compression | `tar -czf` release archives, `zstd -d --long=30` for the LLVM archive | gzip -9, xz -9, zstd 3/19/22, brotli 5/9/11 on the real `fw` ELF (5,677,752 B), a 6,113,607 B wasm and a 21,647,360 B source tar | see the table below | Linux archives: tar + zstd -19 (decode 30 to 76 ms, ratio 2.86 ELF, 6.73 sources); web assets: brotli 11 at build time, zstd at the edge. Live check: `aphrody.com` and `dragonballfr.com` answer `Content-Encoding: zstd` and `alt-svc: h3`. Release asset rename is not done (see remaining) |
| 5 parsing | `serde_json` 1.0.154, `toml` 1.2.2 (plus 0.9.12, 0.8.23, 0.5.11), `toml_edit` 0.25.20 (plus 3 older), `pulldown-cmark` 0.13.6, `zip` 9.8.1 and 8.6.0, `zstd` 0.14.4 and 0.13.5 | sonic-rs 0.5.12, simd-json 0.19.0, toml, toml_edit, comrak 1.1.0 (same host, 11,543-line 7.4 MB jsonl, 880 KB Cargo.lock, 835 KB markdown, median of 7) | see the table below | JSON: `sonic-rs` for read-heavy bulk loads (1.9x serde_json on `Value`); `serde_json` stays where `serde_json::Value` is the API. `simd-json` is slower than `serde_json` here: not adopted. TOML and Markdown: current crates are the fastest of the set. Duplicates found are a manifest job of `crates-map` (aphrody crates on toml 0.9.12 and zstd 0.13.5) |
| 6 build tooling | mold and wild linkers and sccache on the nodes (wild absent on prd, nextest absent on prd), `cargo test` in CI | `cargo nextest` | not measured: the cold build of nextest plus three fw crates on prd ran 25 min at node load 23 to 47 and was stopped to free the node | unchanged; `cargo test` stays until nextest is measured on a warm target (see remaining) |
| 7 CI/CD | `ci.yml` held a Windows job of 1,674 to 2,531 s in the group of all jobs | split workflow, path filters | `gh api` over 7 days: workflow CI 51 runs, 44 cancelled before a job started, 0 success; with `cancel-in-progress: false` on main a pending run is replaced by each newer push. 80 of 1,167 main commits since 2026-11-14 touched only `docs/research` or `docs/agents` | `ci-windows.yml` with its own group and `cancel-in-progress: true`; `ci.yml` Linux jobs unchanged; `paths-ignore` for `docs/research/**` and `docs/agents/**` on both. Linux (cluster) jobs measured in run 44880209087: Rust (linux) 363 s, Kotlin 139 s, Wasm/musl/OCI 94 s, crates.io packaging 59 s, hygiene 35 s |
| 8 containers | root `Dockerfile`: `rust:1.100-alpine` builder, `alpine:3.22` runtime, musl static binary | podman and buildah (present on fr1, absent on prd) | root `.dockerignore` was missing: the build context included `target/` and `node_modules/` | `.dockerignore` added. Distroless/Wolfi, SBOM and provenance not measured (see remaining) |
| 9 browser product | zstd + h3 at the edge (live headers above) | Lighthouse, AVIF, font subsetting | not measured this run | belongs to the web-quality and material-cdn lots (see remaining) |
| 10 agent CLIs | manifest `packages/ai-os/data/tools.toml` (already declares rg, fd, jaq, sd, xh, jj, sccache, nextest, deny, machete, shear, typos, ruff, ty and others) | install state per host against that manifest | missing on fr1: bat, ollama, servoshell, chrome-devtools-mcp, playwright-mcp, apify; missing on prd: 25 entries including cargo-nextest, cargo-deny, cargo-machete, cargo-shear, ruff, ty, typos, oha; missing on the PC: wrangler, cargo-zigbuild, cargo-binstall, cargo-nextest, gix, cargo-deny, cargo-hack and others | No second manifest: the existing one is the source. A legacy gate was added to `configs/claude/retired.toml` (`ci-setup-node`) |

## Compression of real artifacts (prd, 2026-11-21; zstd 1.5.7, brotli 1.2.1, xz 5.8.4, gzip 1.15)

| File | Codec | Output B | Ratio | Compress ms | Decompress ms |
|---|---|---|---|---|---|
| fw ELF 5,677,752 B | gzip -9 | 2,556,359 | 2.22 | 782 | 46 |
| | xz -9 | 1,855,388 | 3.06 | 3,770 | 123 |
| | zstd -3 | 2,379,086 | 2.39 | 128 | 33 |
| | zstd -19 | 1,983,349 | 2.86 | 2,869 | 30 |
| | brotli -q 9 | 2,068,776 | 2.74 | 1,051 | 50 |
| | brotli -q 11 | 1,885,193 | 3.01 | 18,767 | 60 |
| wasm 6,113,607 B | gzip -9 | 2,644,168 | 2.31 | 634 | 41 |
| | xz -9 | 1,863,712 | 3.28 | 4,127 | 115 |
| | zstd -19 | 2,063,593 | 2.96 | 3,759 | 35 |
| | brotli -q 11 | 1,947,642 | 3.14 | 19,651 | 121 |
| sources tar 21,647,360 B | gzip -9 | 4,764,078 | 4.54 | 1,599 | 66 |
| | xz -9 | 3,123,972 | 6.93 | 14,267 | 162 |
| | zstd -19 | 3,218,837 | 6.73 | 12,154 | 76 |
| | brotli -q 11 | 3,087,827 | 7.01 | 44,858 | 98 |

## Parsers (prd, median of 7, release build, thin LTO)

| Task | Crate | Median ms | MB/s |
|---|---|---|---|
| JSON lines to `Value` (7,443,266 B) | serde_json | 14.83 | 501.8 |
| | sonic-rs | 7.84 | 949.2 |
| | simd-json borrowed | 18.59 | 400.3 |
| JSON lines validated only (`IgnoredAny`) | serde_json | 5.16 | 1,443.0 |
| | sonic-rs | 2.91 | 2,560.9 |
| Cargo.lock (880,526 B) | toml `Table` | 19.00 | 46.4 |
| | toml_edit `DocumentMut` | 28.58 | 30.8 |
| | toml_edit `ImDocument` | 20.92 | 42.1 |
| Markdown (834,879 B) | pulldown-cmark events | 3.90 | 214.0 |
| | pulldown-cmark html | 5.83 | 143.3 |
| | comrak html | 6.75 | 123.6 |

## Remaining

- Release archives: `aphrody-<arch>.tar.gz` is named in `.github/workflows/aphrody-release.yml` and in `aphrody/.cloudflare/aphrody-worker/src/releases.ts`; moving to tar.zst needs both and the installer in one commit, owner: release train.
- `cargo nextest` against `cargo test` on a warm target, and nextest, `cargo-deny`, `cargo-machete`, `cargo-shear`, `wild` on prd (25 manifest entries missing there): blocked by prd load 23 to 47 during this run.
- Persistent multiplexed SSH client for the PC (Windows OpenSSH has no ControlMaster): not measured.
- Containers: distroless or Wolfi runtime, SBOM and provenance, podman/buildah on prd.
- Browser product: Lighthouse, AVIF, font subsetting (web-quality and material-cdn lots).
- Duplicate crate versions (toml 0.9.12 vs 1.2.2, zstd 0.13.5 vs 0.14.4, zip 8.6.0 vs 9.8.1, brotli 7.0.0 vs 9.1.3) and `retired.toml` failures that predate this lot (agy-tooling, relay-hub hits in `docs/research/pure-rust-baseline.txt` and `Cargo.lock`): other lots.
