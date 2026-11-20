# ay-rag/localstore

The single agent-home database (`<home>/db/aphrody.sqlite`): per-profile path resolution, versioned migrations, source and file fingerprints, pipeline journal, sync state and checksum-verified backups. Ships the `ay-rag/localstore` binary.

Legacy name: this unit was published on crates.io as `aphrody-store`. It is restored here as `ay-rag/localstore` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/ay-rag` |

```sh
cargo test -p ay-rag/localstore
cargo clippy -p ay-rag/localstore --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
