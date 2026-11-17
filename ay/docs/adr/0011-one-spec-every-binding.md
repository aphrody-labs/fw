# ADR-0011: One spec for the ABI, every binding generated, tree-sitter as the checker

Status: accepted, 2026-11-17 (owner request: find how to generate Bun types, an ABI, an FFI, an OpenAPI, a spec, a standard and a
complete tree-sitter over a very large Rust, C, Python and assembly monorepo, the way a kernel is kept coherent).

## Context
ADR-0008 says "the stable boundary is the C ABI (headers generated from one spec)". On 2026-11-17 that spec did not exist:
`corelib/include/ay_abi.h` was hand-written and called itself the source of truth, `corelib/wit/ay-corelib.wit` and
`packages/ffi` were hand-written next to it, and nothing checked that the corelib implemented what the header declared.

Precedents read in this session:
- Bun's own bindgen: every `*.bind.ts` under `src/` is read by `src/codegen/bindgen*.ts` and generates the Rust, the C++ and
  the TypeScript types of one binding (`docs/project/bindgen.mdx` of the fork). One typed source, three outputs.
- The Linux kernel keeps C headers as the ABI and derives Rust from them with bindgen (`rust/bindgen_parameters`,
  `rust/bindings`, `rust/helpers`); its scripts for scale are `generate_rust_analyzer.py`, `clang-tools`, `coccicheck`,
  `checkpatch.pl`, `get_maintainer.pl`, `kernel-doc`, `rust_is_available.sh` (listing of `scripts/` and `rust/` read with the
  GitHub API).
- Tools that exist and are maintained (GitHub API, pushed dates 2025-09 to 2026-11): tree-sitter 0.27.2, SCIP (Sourcegraph),
  Kythe (Google), cbindgen, rust-bindgen, Coccinelle; `github/stack-graphs` is archived (2025-09-09).

## Decision
1. **The spec is the source** (`abi/ay.abi.toml`): constants with their syscall arities, functions with typed signatures, the
   wasm import. It is typed data, not code in any one language, so no binding is privileged.
2. **Everything else is generated** by `tools/abi.ts`: the C header, Rust constants and `extern "C"` blocks, the Bun module
   (`bun:ffi` symbol table, constants, syscall arities), Python ctypes signatures, a JSON form carrying the spec's SHA-256,
   and the human spec `docs/abi.md`.
3. **Tree-sitter checks it, per language** (`tools/ts.ts`, `web-tree-sitter` with the grammar packages' own wasm): each generated
   file must parse without error and carry exactly the spec's constants and signatures, and the corelib sources must define
   exactly the spec's exported functions with the same signatures and the same per-syscall argument counts (read from the
   `ay_arg_count` switch). `bun tools/abi.ts --check` is part of `bun run check`.
4. **The migration was proved by round trip**: the hand-written header and the generated one were both parsed with tree-sitter
   and compared: 22 constants and 8 functions each, 0 differences.

## Proof and limits (measured 2026-11-17)
- 6 tests (`tools/abi.test.ts`) pass, including drift detection: a changed constant, a changed signature, an exported function
  missing from the spec and a wrong syscall arity are each reported.
- The grammar packages `tree-sitter-c` 0.24.1 (ABI 15), `tree-sitter-rust` 0.24.0 (ABI 14) and `tree-sitter-python` 0.25.0
  (ABI 15) ship wasm that `web-tree-sitter` 0.27.2 loads under Bun; the older `tree-sitter-wasms` 0.1.13 bundle did not load and
  was not kept.
- Known limits, handled in the checker and written here so they are not rediscovered: the C grammar cannot parse a preprocessor
  conditional that splits a declaration (the wasm import attribute is removed before parsing the header); `tree-sitter-rust`
  0.24.0 does not parse `unsafe extern` blocks (read as `extern`); an attribute macro (`AY_API`) is blanked before parsing the
  corelib; Bun's FFI maps `usize` to a 64-bit integer.
- **Not done, not proved**: an assembly grammar (none ships wasm; `RubixDev/tree-sitter-asm` exists and must be built on the
  build host); structs, enums and opaque handles in the spec; generating `ay_arg_count` itself from the spec; OpenAPI and WIT
  generation from the same spec (the WIT world is still hand-written); a run of any of this on a kernel-sized tree. Tree-sitter
  gives syntactic facts; names resolved across translation units need a compile database (`compile_commands.json`,
  `rust-project.json`) and an index format such as SCIP.

## Next
1. Spec types for structs, enums, handles and error sets; generate `ay_arg_count` and the WIT world from the spec.
2. Build the assembly grammar to wasm on the build host and add `asm` to `tools/ts.ts`.
3. A repository-wide fact store (SQLite) fed by tree-sitter for Rust, C, Python and assembly, with the compile databases for
   resolution, benchmarked on a Linux-sized tree on the build host before any claim of scale.
4. `ffi` call tests: build the corelib as a shared library on the build host and call every symbol through the generated Bun
   and Python modules.
