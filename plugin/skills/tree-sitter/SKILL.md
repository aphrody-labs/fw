---
name: tree-sitter
description: Authoritative Tree-sitter 0.27+ incremental parsing, AST queries, highlighting, tags, and multi-language Rust/C ABI integration for Aphrody and Fairwind. Use when embedding tree-sitter parsers, writing S-expression queries, extracting symbols, or bridging AST analysis over FFI.
---

# Tree-sitter Engineering Reference (Verified 2026-11-19)

Live-verified against `https://github.com/orgs/tree-sitter/repositories` (48 active repos) and `https://crates.io/api/v1/crates/<name>`.

## Verified Crate Versions

| Crate | Version | Role |
| --- | --- | --- |
| `tree-sitter` | `0.27.2` | Core incremental parser & query engine |
| `tree-sitter-highlight` | `0.27.2` | Syntax highlighting engine |
| `tree-sitter-tags` | `0.27.2` | Symbol/definition/reference extraction |
| `tree-sitter-rust` | `0.24.2` | Rust grammar |
| `tree-sitter-typescript` | `0.23.2` | TypeScript / TSX grammar |
| `tree-sitter-python` | `0.25.0` | Python grammar |
| `tree-sitter-c` | `0.24.2` | C grammar |
| `tree-sitter-cpp` | `0.23.4` | C++ grammar |
| `tree-sitter-java` | `0.23.5` | Java grammar |
| `tree-sitter-json` | `0.24.8` | JSON grammar |
| `tree-sitter-bash` | `0.25.1` | Bash grammar |

## Integration Patterns

1. **Rust `Parser` setup**:
   - Instantiate `tree_sitter::Parser::new()`.
   - Bind grammar via `parser.set_language(&tree_sitter_rust::LANGUAGE.into())?`.
   - Parse with `parser.parse(source, None)` and walk `tree.root_node()` or run `tree_sitter::Query`.
2. **C ABI / `bun:ffi` single-binary bridge**:
   - Export `#[unsafe(no_mangle)] pub extern "C" fn` entry points from a `cdylib` crate (`fairwind-abi`).
   - Pass UTF-8 `(ptr, len)` slices; return counts, error flags, or caller-freed JSON buffers.
   - Invoke from Bun (`bun:ffi` `dlopen`) inside `bun build --compile --bytecode` single binaries.
3. **Verification**:
   - Always verify with `cargo check`, `cargo test`, and a live CLI AST parse on real source files (`exit 0`).
