# ay-ml/jobs

Persistent background job queue on SQLite: submission, leased claims, crash recovery, retries and NDJSON logs.

Legacy name: this unit was published on crates.io as `aphrody-jobs`. It is restored here as `ay-ml/jobs` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/ay-ml` |

```sh
cargo test -p ay-ml/jobs
cargo clippy -p ay-ml/jobs --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
