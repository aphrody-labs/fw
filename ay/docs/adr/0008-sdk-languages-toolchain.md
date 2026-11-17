# ADR-0008: SDK languages and toolchain

Status: accepted, 2026-11-17 (owner decision).

## Context
`aphrody-dev/aphrody` is the complete OS in pure Rust and the source of truth. This repository is its SDK:
C ABI, OpenAPI, docs, spec, standards, corelib, libc and the design system. One checkout per repository,
one repository per unit, no duplicated code across the ecosystem.

## Decision
- **Languages accepted here**: C and its derivatives (C++, Objective-C/C++, CUDA/HIP C++), assembly
  (x86-64, AArch64, RISC-V, WebAssembly text), Zig, WebGPU (WGSL), Python, .NET (C#, F#) and Bun
  (TypeScript/JavaScript), and Rust. Amended 2026-11-17: the earlier Rust exclusion is withdrawn; each
  unit uses the best-fit language.
- **LLVM first**: clang/clang++/clang-cl, lld (ld.lld, lld-link, wasm-ld), llvm-ar, llvm-objcopy, libc++ and
  compiler-rt; Zig and .NET NativeAOT ride the same LLVM backend where they can. One toolchain for every target.
- **Tree-sitter** is the parser layer for every language the SDK reads (listings, docs, bindings, lint).
- **ABI and FFI**: the stable boundary is the C ABI (headers generated from one spec); every other
  language binds to it (Bun `bun:ffi`, Python `ctypes`/cffi, .NET `LibraryImport`, Zig `@cImport`, C++ directly,
  Rust through `extern "C"`). WebAssembly gets the same ABI as imports/exports and a WIT world.
- **Build modes**: LTO (ThinLTO by default, full LTO for releases), AOT everywhere (NativeAOT for .NET,
  Bun compile, Zig/C native), PGO when a benchmark exists.
- **Unified lib / MSVC**: one source tree builds with clang on Linux/macOS and clang-cl + lld-link against the
  MSVC/UCRT ABI on Windows; the same corelib is exported as static, shared and wasm libraries.
- **Cross everything**: cross-arch (x86-64, AArch64, RISC-V, wasm32/wasm64), cross-OS (Linux, Windows,
  macOS, Android, freestanding/bare metal, WASI and the browser), cross-GPU (WebGPU/WGSL as the portable
  layer over Vulkan, D3D12 and Metal) and cross-platform packaging, all from the target list in `ay.toml` [sdk].

## Consequences
- Versions are read live (LLVM, Zig, .NET, Python, tree-sitter grammars) and never pinned in prose.
- A target is supported only when CI builds it and a test runs on it (native host, QEMU, Wasm runtime or browser).
