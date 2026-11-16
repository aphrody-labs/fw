# ADR-2: Nix for toolchains and packages, native workspaces for code, one LLVM

Status: accepted, 2026-11-16.

## Decision
- **Nix flake** pins every toolchain (Rust, Bun, LLVM, Go, .NET), provides the dev shells, runs CI checks, and
  builds the OS packages (ADR-4).
- **Native workspaces** build code: one Cargo workspace (`crates/*`, `[workspace.dependencies]`) and one Bun
  workspace (`packages/*`, `apps/*`). Other languages get their own conventional root when their first unit lands.
- **`ay` typed scripts** in `tools/` orchestrate; no ad-hoc command sequences.
- **One LLVM toolchain**:
  - C/C++: clang and lld, `-flto=thin` at compile and link; cache `-Wl,--thinlto-cache-dir=<dir>` (ELF) or
    `/lldltocache:<dir>` (COFF); jobs `-Wl,--thinlto-jobs=N`.
  - Rust: same LLVM, `-C linker-plugin-lto` for cross-language LTO through lld; release `lto = "thin"`,
    `codegen-units = 1`.
  - Bun: `bun compile` (= `bun build --compile --aot`) for typed code built on its target platform; `--no-aot` for
    cross targets or untyped hot paths.
  - GN/ninja upstreams (Chromium): depot_tools `gclient` and `autoninja` on the build host.
- **tree-sitter** is the one parsing layer for code intelligence (craft, search over code, agents).

## Not chosen
Buck2 or Bazel: considered only when a measured need (remote execution, cross-language incremental builds the
native tools can't do) appears.
