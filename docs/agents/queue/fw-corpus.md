# Queue: fw-corpus (Google design, Android, Chromium, GitHub, Kotlin, Rust, Cargo, Compose Multiplatform)

## Goal

Turn design.google, m3.material.io, developer.android.com, source.android.com, chromium and android googlesource, GitHub docs and the aphrody org inventories, Kotlin, Rust (book, reference, rustc dev guide, std rustdoc JSON), Cargo and Compose Multiplatform into a searchable local corpus on fr1 (`/srv/xbuild/corpus/`, per-source SQLite FTS5), reachable through `fw learn search` (`corpusHits`) and `fw corpus ...`; then cited skills in `plugins/aphrody/skills/`, an auto-refresh timer, and five sample Q&A with citations.

## Done (commits on origin/main, see the reply for SHAs)

- `crates/fw-corpus` (standalone crate with its own `[workspace]` and Cargo.lock, no root workspace edit): store (doc and chunk tables, external-content FTS5, bm25, prune), chunker, polite HTTP client (per-host pacing, retry with Retry-After, robots.txt with longest-match, ETag/Last-Modified), HTML to markdown, Chrome dump-dom renderer, sitemap adapter (HTML, DevSite `.md.txt`, Chrome), gitiles adapter (HTML repo index, commit JSON, tree and blob via `?format=TEXT`), GitHub adapter (git trees API, raw at the pinned commit, org inventories), rustdoc JSON adapter (std, core, alloc), source registry, CLI (`sources|status|sync|search|read|systemd`).
- Wiring in fw-bin: `fw corpus` runs the sibling `fw-corpus`; `fw learn search` adds `corpusHits`. These fw-bin hunks were NOT compiled with fw-bin (fr1 was saturated); they are 20 lines, run `cargo check -p fw-bin` first.
- Verified on fr1 (release build, 11m49s on a loaded host): `cargo test --release` 12 passed; live smoke `fw-corpus sync cargo --limit 20` and `rust-std --limit 20` stored docs (cargo 20 docs / 708230 bytes from `https://doc.rust-lang.org/cargo/...`; rust-std 20 items with `nightly/std/...` URLs) and the FTS query path ran.

## Per-source fetch state

Nothing is crawled yet (smoke DBs removed). `/srv/xbuild/corpus/` is empty. Every source is resumable: re-running `sync` skips docs whose stored rev (sitemap lastmod, blob sha, commit-pinned sha, ETag, rustc version) is unchanged.

| source | adapter | state | notes |
| --- | --- | --- | --- |
| design-google | sitemap, HTML | not run | sitemap 296 URLs, SSR HTML (152 KB pages) |
| m3 | sitemap, Chrome | not run | 385 URLs, SPA; needs `google-chrome` (present on fr1); robots disallows `/archive`, `?q=` |
| android-dev | sitemap, `.md.txt` | not run, live smoke timed out at 80 s | `<page>.md.txt` returns markdown (verified 200 text/markdown on /about/versions/16/behavior-changes-all); sitemap index has 10 children, ~1.3 MB each, intermittent HTTP 500 with empty body (the 0 `<loc>` seen earlier), the adapter retries 4 times; locale `?hl=` URLs are dropped; API reference excluded by the prefix filter in `sources.rs` (`android_dev`) |
| android-source | sitemap, `.md.txt` then HTML | not run | `.md.txt` returned 404 for one guessed page; HTML fallback is coded, unverified live; 5 sitemap children |
| chromium-repos, aosp-repos | gitiles inventory | not run | root `?format=TEXT` is disallowed by robots, so the HTML index is parsed (1992 items counted in the chromium HTML); 3 requests per repo, ~1h each at 250 ms/host; `platform/docs/source.android.com` holds only README.txt |
| chromium-docs, aosp-docs | gitiles walk | not run | file reads under `+/<sha>/path?format=TEXT` are allowed by robots; `+archive` and `+log` are disallowed and unused |
| github-docs | GitHub trees | not run | `content/` has 3805 .md at HEAD |
| github-orgs | gh api | not run | uses `GH_TOKEN`, else `gh auth token` (fr1 logged in as aphrody-dev); stays on fr1 (includes private repo metadata) |
| kotlin | GitHub trees | not run | `docs/topics/` 307 .md |
| compose-multiplatform | GitHub trees | not run | compose-multiplatform 65 .md, kotlin-multiplatform-dev-docs `topics/` 131, compose-hot-reload 8 |
| rust | GitHub trees | not run | book 112, reference 121, rustc-dev-guide 251, nomicon 64, edition-guide 56 |
| cargo | GitHub trees | smoke ok | the book moved: prefix is `doc/book/src/` (99 .md), not `src/doc/src/` |
| rust-std | rustdoc JSON | smoke ok | `rustup component add rust-docs-json --toolchain nightly` already installed on fr1 (std 19 MB, core 99 MB, alloc 9 MB json); nightly `rustc 1.102.0-nightly (c7d5ba59d 2026-11-20)` |

## Remaining

1. `cargo check -p fw-bin` and `cargo clippy -p fw-corpus -- -D warnings` (not run).
2. Build on fr1 and install: `cd ~/src/fw/crates/fw-corpus && CARGO_TARGET_DIR=/srv/xbuild/fw-corpus-target TMPDIR=/srv/xbuild/tmp cargo build --release -j4`, then `install -m755 /srv/xbuild/fw-corpus-target/release/fw-corpus /usr/local/bin/` (fw is at `/usr/local/bin/fw`, the sibling is found next to it or on PATH).
3. Crawl in this order (small and certain first): `fw-corpus sync cargo rust rust-std kotlin compose-multiplatform github-docs github-orgs`, then `android-dev`, `m3`, `design-google`, `android-source`, `chromium-docs aosp-docs`, last `chromium-repos aosp-repos`. Keep `--threads 6 --rate-ms 250` (defaults); gitiles may answer 429, the client honours Retry-After.
4. Inspect real DOM of one m3 page (`google-chrome --headless=new --dump-dom`) and tune the `<main>` extraction in `html.rs` if the rendered text is thin.
5. Install the refresh timer: `fw-corpus systemd --bin /usr/local/bin/fw-corpus` prints a service and a timer (daily 03:30, Persistent, Nice 19); split at the `# fw-corpus-refresh.timer` marker into `/etc/systemd/system/fw-corpus-refresh.{service,timer}` with `Environment=TMPDIR=/srv/xbuild/tmp`, then `systemctl enable --now fw-corpus-refresh.timer`.
6. Skills: router skill `plugins/aphrody/skills/corpus/SKILL.md` plus distilled, cited references added to the existing `google-design`, `android`, `kotlin-compose-m3`, `rust-cargo` and `chromium` skills (no verbatim Google pages; each reference cites URL or commit plus read date).
7. Five sample Q&A from `fw-corpus search` with URLs: an M3 component spec, an Android 17 behaviour change (`/about/versions/17/...`), a Chromium `docs/` design doc, the Cargo feature resolver (`reference/resolver.html`, `reference/features.html`), a Compose Hot Reload setup step.
8. Optional R2 copy of the `.db` files with wrangler.

## Resume commands

```
ssh fr1
cd ~/src/fw && git pull   # picks up crates/fw-corpus
fw-corpus status
fw-corpus sync cargo rust --root /srv/xbuild/corpus
fw-corpus search "feature resolver" --limit 5
fw learn search "feature resolver"      # after fw-bin is rebuilt with the wiring
```

## Directories created

- `/srv/xbuild/corpus/` on fr1: empty now, final home of the corpus (expect well under 1 GB of text; android-dev dominates). Keep.
- `/srv/xbuild/tmp/` on fr1: 136 KB, build log and TMPDIR for these builds; safe to delete if nothing else uses it.
- `/srv/xbuild/fw-corpus-target/` on fr1: was 158 MB, deleted by someone else's disk cleanup; rebuild takes ~12 min on a loaded host. Safe to delete.
- `/srv/xbuild/src/fw/crates/fw-corpus` (also `~/src/fw/crates/fw-corpus`): a copy of the crate synced by tar, same as git. Safe to delete after `git pull`.
- `~/.rustup/toolchains/nightly-*/share/doc/rust/json/` on fr1: 130 MB from `rust-docs-json`; required by `rust-std`.
- PC: `~/AppData/Local/Temp/fw-push` (clone used to push, safe to delete), probe files in `~/AppData/Local/Temp/` (`sitemap_*_of_10.xml`, `aurls.txt`, `chr.html`, `m3.html`).
