# Queue: fw-corpus (Google design, Android, Chromium, GitHub, Kotlin, Rust, Cargo, cross toolchains, LLVM, Compose Multiplatform)

## Goal

Turn design.google, m3.material.io, developer.android.com, source.android.com, chromium and android googlesource, GitHub docs and the aphrody org inventories, Kotlin, Rust (book, reference, rustc book, rustc dev guide, std rustdoc JSON, Forge, RFCs, goals), Cargo, the cross toolchains (cross, cargo-zigbuild, cargo-xwin, xwin), the LLVM project docs and Compose Multiplatform into a searchable local corpus on fr1 (`/srv/xbuild/corpus/`, per-source SQLite FTS5), reachable through `fw learn search` (`corpusHits`) and `fw corpus ...`; then cited skills in `plugins/aphrody/skills/`, an auto-refresh timer, and five sample Q&A with citations.

## Done

- `crates/fw-corpus` (standalone crate with its own `[workspace]` and Cargo.lock): store, chunker, polite HTTP client, HTML to markdown, Chrome renderer, sitemap, gitiles, GitHub and rustdoc JSON adapters, 18 sources (`fw-corpus sources`), CLI `sources|status|sync|search|read|systemd`.
- Built on prd (builduser, `/srv/build/corpus-target`), `cargo clippy --all-targets -D warnings` clean, 13 tests pass; binary installed on fr1 at `/usr/local/bin/fw-corpus` (same x86_64 and glibc 2.44 on both nodes).
- `cargo check -p fw-bin` on prd against origin/main (workspace unified with aphrody and ay): `Finished`, so the `fw corpus` and `corpusHits` wiring compiles.
- Fixes found by crawling: every adapter writes each document as it arrives (a killed crawl kept nothing before); gitiles skips an unlistable directory instead of aborting the source, 500 ms pacing and longer 429 backoff; HTML conversion drops attributes over 20 KB (m3 ships a 26 MB token attribute per page) and icon or copy-link chrome; `sync --only <url text>` fetches one section of a sitemap source; `github-orgs` lists personal accounts through `user/repos`; nested prefixes (`llvm/docs/`) list their own subtree because the monorepo tree is truncated.
- New sources: `rust-process` (Forge, RFCs, goals), `rust-cross` (cross, cargo-zigbuild, cargo-xwin, xwin), `llvm` (LLVM, Clang, lld, MLIR); `rust` gained the rustc book, rustdoc book, unstable book and style guide.
- Refresh timer on fr1: `/etc/systemd/system/fw-corpus-refresh.{service,timer}` (User=ubuntu, TMPDIR=/srv/xbuild/tmp, `sync all`, daily 03:30 with 20 min jitter), enabled; `systemd-analyze verify` passes.
- Skills: `plugins/aphrody/skills/corpus` (router, `references/sample-qa.md` with the five answered questions), cited references `corpus-rust-cargo.md`, `corpus-google-design.md`, `corpus-android.md`, `corpus-kotlin-compose.md`, `corpus-chromium.md` in the existing skills.

## Per-source state (measured on fr1 with `fw-corpus status`)

| source | docs | state |
| --- | --- | --- |
| cargo, rust, rust-std, rust-process, rust-cross, llvm, kotlin, compose-multiplatform, github-docs, github-orgs, aosp-docs | complete | last sync report failed 0 |
| android-dev | 5200 of 5205 | 5 URLs answer HTTP 404 (`___` partial pages) |
| design-google | 269 of 296 | 27 library URLs answer HTTP 403 (checked ones redirect to medium.com) |
| android-source | in progress | `fw-corpus sync android-source` running detached (log `/srv/xbuild/corpus/logs/batch10.log`) |
| m3 | in progress, 385 sitemap URLs | about one page a minute on a load-80 host; `--only /components/` first, then the rest (`logs/batch6.log`) |
| chromium-docs | 749, walk of `third_party/blink/renderer` in progress | first run aborted on HTTP 429 (fixed); rerun in `logs/batch9.log` |
| aosp-repos, chromium-repos | in progress | `logs/batch8.log`; chromium-repos starts after aosp-repos |

All syncs are resumable and write per document: a run that stops keeps its documents, and the daily timer finishes the rest.

## Remaining

1. Let the detached crawls finish (m3, chromium-docs, aosp-repos, chromium-repos, android-source), then `fw-corpus status` should show `failed 0` apart from the two lists above; fr1 load was about 80 during this run, so Chrome rendering of m3 is the long pole.
2. Extend the cited references with the pieces still filling (m3 foundations and styles, chromium Blink READMEs, source.android.com, AOSP) once those stores are complete.
3. Optional: R2 copy of the `.db` files with wrangler (not started; the corpus is 0.4 GB of SQLite).
4. Optional: expose `fw corpus search` as an MCP tool of the plugin server `fw`.

## Resume commands

```
ssh fr1
fw-corpus status
fw-corpus sync m3 --only /components/ --threads 4
fw-corpus search "feature resolver" --source cargo --limit 5
fw-corpus read "components/buttons/specs" --source m3
systemctl list-timers fw-corpus-refresh.timer
```

## Directories

- fr1 `/srv/xbuild/corpus/` (databases, `logs/batch*.log`): final home, keep.
- fr1 `/srv/xbuild/tmp/`: TMPDIR for crawls and the Chrome profile directories.
- prd `/srv/build/corpus-src`, `/srv/build/corpus-target`, `/srv/build/fw-check`, `/srv/build/fw-check-target`: build trees for this lot, safe to delete.
