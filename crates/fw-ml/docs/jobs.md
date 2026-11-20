# fw-ml/jobs

Persistent background job queue on SQLite: submission, leased claims, crash recovery, retries and NDJSON logs.

Legacy name: this unit was published on crates.io as `aphrody-jobs`. It is restored here as `fw-ml/jobs` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/fw-ml` |

```sh
cargo test -p fw-ml/jobs
cargo clippy -p fw-ml/jobs --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-labs/fw](../../README.md).
