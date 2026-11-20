# fw-translate-core

Pure stages of the comment rewriter: comment extraction, AI-marker scrubbing, prose style and language detection. No network, no runtime, wasm32-safe.

Legacy name: this unit was published on crates.io as `aphrody-translate-core`. It is restored here as `fw-translate-core` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/fw-translate-core` |

```sh
cargo test -p fw-translate-core
cargo clippy -p fw-translate-core --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-labs/fw](../../README.md).
