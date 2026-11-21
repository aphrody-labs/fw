---
name: corpus
description: "Search the local docs corpus on fr1 (design.google, Material 3, developer.android.com, source.android.com, Chromium and AOSP docs, GitHub docs, Kotlin, Compose Multiplatform, Rust book/reference/rustc guide, Cargo, std rustdoc) with `fw corpus` and `fw learn search`. Use before answering from memory about any of those"
---

# Docs corpus (`fw corpus`)

One SQLite FTS5 store per source under `/srv/xbuild/corpus/` on `fr1` (`<source>.db`), filled by the sibling binary `fw-corpus` (crate `crates/fw-corpus`). Ask the corpus first, then cite what it returned; an answer about Material 3, Android, Chromium, Kotlin, Rust or Cargo that was not checked here or on the live page is a guess.

## Commands (run on fr1, or `ssh fr1 fw-corpus ...`)

- `fw-corpus sources`: ids, titles and licences, derived from the registry in `crates/fw-corpus/src/sources.rs`; never copy the list into a doc.
- `fw-corpus status`: per source documents, chunks, bytes, last fetch and the last sync report (`fetched`, `unchanged`, `failed`, `errors`).
- `fw-corpus search "<words>" [--source ID] [--limit N]`: bm25 over all stores or one; each hit has `url`, `heading`, a `snippet` (query terms in `[brackets]`), `pinned` (commit-pinned URL for GitHub sources) and `fetched`.
- `fw-corpus read <url-substring> [--source ID]`: the full stored markdown of one document.
- `fw-corpus sync <id...|all> [--limit N] [--force]`: polite incremental crawl (robots.txt, 250 ms per host, Retry-After, ETag and lastmod, resumable); unchanged documents are skipped.
- `fw learn search "<words>"` adds `corpusHits` to the code and history hits; `fw corpus ...` forwards to `fw-corpus`.

## Refresh

`fw-corpus-refresh.timer` on fr1 runs `fw-corpus sync all` daily (unit text from `fw-corpus systemd --run-as <user> --tmpdir <dir> --root <dir>`). `systemctl list-timers fw-corpus-refresh.timer` shows the next run; `journalctl -u fw-corpus-refresh` the last report.

## Citing and licences

- Cite the `url` (and `pinned` when present) plus the `fetched` date; a distilled reference states them in its header.
- Google pages (design.google, m3.material.io) are for local search: paraphrase in skills, never paste pages. Android and Chromium text carries CC BY 2.5, Apache-2.0 and BSD-3-Clause; the licence of a source is in `fw-corpus sources`.
- `github-orgs` holds metadata of private repositories: it stays on fr1 and is never quoted in public files.

## Where the distilled references live

- `google-design/references/corpus-google-design.md` (design.google, Material 3)
- `android/references/corpus-android.md` (developer.android.com, source.android.com, AOSP)
- `kotlin-compose-m3/references/corpus-kotlin-compose.md` (Kotlin, Compose Multiplatform)
- `rust-cargo/references/corpus-rust-cargo.md` (Rust book, reference, rustc guide, std, Cargo)
- `chromium/references/corpus-chromium.md` (chromium/src docs)
- `corpus/references/sample-qa.md`: five questions answered from the corpus with their citations
