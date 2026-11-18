# ay-code-graph

Native code graph (nodes, EXTRACTED and INFERRED edges with confidence, communities, god nodes, query, path and explain, report, graph.json) stored in the home database. Own implementation of the Graphify concepts (Apache-2.0, see NOTICE). Ships the `ai-os-graph` binary.

Legacy name: this unit was published on crates.io as `aphrody-code-graph`. It is restored here as `ay-code-graph` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/ay-code-graph` |

```sh
cargo test -p ay-code-graph
cargo clippy -p ay-code-graph --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
