# ay-agent/guard

Safety guardrails for autonomous agents: pre-main process hardening and a shell command-safety classifier (Allow, Prompt, Forbidden).

Legacy name: this unit was published on crates.io as `aphrody-guard`. It is restored here as `ay-agent/guard` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/ay-agent` |

```sh
cargo test -p ay-agent/guard
cargo clippy -p ay-agent/guard --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
