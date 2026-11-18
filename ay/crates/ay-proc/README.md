# ay-proc

Process supervision primitives: process groups that die with their owner, detached daemons, liveness probes, elevated execution, free ports, advisory file locks and the single-GPU lease.

Legacy name: this unit was published on crates.io as `aphrody-proc`. It is restored here as `ay-proc` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/ay-proc` |

```sh
cargo test -p ay-proc
cargo clippy -p ay-proc --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
