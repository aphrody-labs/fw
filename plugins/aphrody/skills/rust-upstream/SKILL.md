---
name: rust-upstream
description: "Live Rust, Cargo, libc, rustup, rust-analyzer, Clippy, Miri, Cranelift, Tree-sitter, Zed AST, Wild, Mold, and sccache knowledge brain via Agent Cards, TSV matrix"
---
> , and 64-dim vector index. Refresh anytime via just rust-brain (bun tools/rust_brain.ts).


# Rust Upstream & Toolchain Brain (`fw`)

- **Local Toolchain**: `rustc 1.102.0-nightly (3ef0c22cd 2026-11-11)` (LLVM version: 23.1.5), `cargo 1.102.0-nightly (c07ac2c4d 2026-11-06)`, `rustup 1.29.3 (edbc831ce 2026-11-02)`, `rust-analyzer 1.102.0-nightly (3ef0c22cd 2026-11-11)` (336 targets, 20 installed).
- **Artifacts** (refresh in parallel via `just rust-brain`):
  - Matrix (16 upstreams): `docs/rust-brain/RUST_UPSTREAM_MATRIX.tsv`
  - A2A Agent Cards (9 toolchains + 180 workspace AST symbols): `docs/rust-brain/agent-cards.json`
  - Vector Index (39 L2-normalized 64-dim vectors): `docs/rust-brain/vectors.jsonl`
- **Instant Vector Search**: `bun tools/rust_brain.ts search "<query>"`

| Upstream | Domain | Stars | Release | Role |
| :--- | :--- | ---: | :--- | :--- |
| `rust-lang/rust` | compiler | 122242 | `1.100.0` | rustc, std, core, alloc, MIR, LLVM/Cranelift codegen |
| `rust-lang/cargo` | build | 15754 | `rolling` | Package manager, workspace resolver v2, -Zscript, xtask, crates.io |
| `rust-lang/libc` | ffi | 2648 | `0.2.191` | Raw C/POSIX/Linux/Win32/musl platform FFI bindings |
| `rust-lang/rustup` | toolchain | 7129 | `rolling` | Toolchain, target, and component manager (336 targets) |
| `rust-lang/rust-analyzer` | ast-lsp | 17009 | `2026-11-16` | Incremental IDE compiler front-end, HIR/MIR, symbol/type graph |
| `rust-lang/rust-clippy` | lint | 13681 | `rolling` | Static analysis & clippy::perf zero-allocation enforcement |
| `rust-lang/miri` | ub-check | 6755 | `rolling` | MIR interpreter for Stacked/Tree Borrows & UB verification |
| `rust-lang/rustc_codegen_cranelift` | codegen | 2175 | `rolling` | Fast dev codegen backend (2x-5x faster compile than LLVM) |
| `rust-lang/cc-rs` | ffi-build | 2169 | `cc-v1.11.2` | Build-script C/C++/Asm compiler driver |
| `rust-lang/rust-bindgen` | ffi-gen | 5327 | `v0.73.2` | C/C++ header to Rust FFI generator |
| `rust-lang/rfcs` | spec | 6646 | `rolling` | Language, Cargo, Edition 2024, and standard library RFCs |
| `tree-sitter/tree-sitter-rust` | ast-cst | 539 | `v0.24.2` | Incremental concrete syntax tree grammar (used by fw C ABI & Zed) |
| `zed-industries/zed` | editor-ast | 95320 | `v1.29.2` | GPUI + Tree-sitter + rust-analyzer multi-buffer AST engine |
| `davidlattimore/wild` | linker | 4125 | `0.11.0` | Ultra-fast Rust linker for Linux ELF & WebAssembly |
| `rui314/mold` | linker | 17762 | `v3.0.0` | Modern parallel linker in Rust (v3.0+) |
| `mozilla/sccache` | cache | 7952 | `v0.19.0` | Shared local/remote Rust/C/C++ compilation cache |
