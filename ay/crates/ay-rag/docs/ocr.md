# ay-rag/ocr

Portable, versioned OCR result contract with provenance and quality states, DocTags parsing and an optional SQLite store (feature `sqlite`).

## Restoration

Legacy name: this unit was published on crates.io as `aphrody-ocr-core`. It is restored here as `ay-rag/ocr` (`publish = false`; nothing is published from this repository by hand).

| Type | Path |
| :-- | :-- |
| Rust crate | `crates/ay-rag` |

```sh
cargo test -p ay-rag/ocr
cargo clippy -p ay-rag/ocr --all-targets -- -D warnings
```

Rust builds run on the build host or in CI, not on a workstation.

Part of Aphrody: [aphrody-dev/ai-os](../../README.md).
