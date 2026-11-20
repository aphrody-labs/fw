# fw-agent/rollout

Resilient agent-session recorder: an async actor appends timestamped JSONL items to a rollout file and a prior session is resumed by replaying it.

Legacy name: this unit was published on crates.io as `aphrody-rollout`. It is restored here as `fw-agent/rollout` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/fw-agent` |

```sh
cargo test -p fw-agent/rollout
cargo clippy -p fw-agent/rollout --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-labs/fw](../../README.md).
