# fw-secrets

Secret store abstraction (environment, plaintext file, AES-256-GCM with Argon2id encrypted file) with redacted formatting, constant-time equality and zeroize-on-drop values.

Legacy name: this unit was published on crates.io as `aphrody-secrets`. It is restored here as `fw-secrets` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/fw-secrets` |

```sh
cargo test -p fw-secrets
cargo clippy -p fw-secrets --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
