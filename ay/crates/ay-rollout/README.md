# ay-rollout

Resilient agent-session recorder: an async actor appends timestamped JSONL items to a rollout file and a prior session is resumed by replaying it.

Legacy name: this unit was published on crates.io as `aphrody-rollout`. It is restored here as `ay-rollout` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/ay-rollout` |

```sh
cargo test -p ay-rollout
cargo clippy -p ay-rollout --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
