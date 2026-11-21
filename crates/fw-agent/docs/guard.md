# fw-agent/guard

Safety guardrails for autonomous agents: pre-main process hardening and a shell command-safety classifier (Allow, Prompt, Forbidden).

Legacy name: this unit was published on crates.io as `aphrody-guard`. It is restored here as `fw-agent/guard` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/fw-agent` |

```sh
cargo test -p fw-agent/guard
cargo clippy -p fw-agent/guard --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-labs/fw](../../../README.md).
