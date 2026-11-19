---
name: low-level-lto-aot
description: Build and verify low-level Fat-LTO and AOT binaries across Assembly (x86-64/ARM64/RISC-V), WebAssembly (wasm32/WASI), CUDA/NVPTX64, WebGPU/WGSL, Rust, C, C++, Go, Java, Kotlin, Python, and Bun for every architecture inside Fairwind.
---

# Fairwind Low-Level Cross-Language & Cross-Architecture LTO/AOT Guide

Use `fairwind lowlevel matrix` and `fairwind lowlevel verify` to inspect and verify real native/GPU/Wasm builds across every supported architecture.

## 1. Compiler & Flag Matrix

- **Rust (`x86_64`, `aarch64`, `riscv64`, `wasm32-wasip2`)**: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`.
- **C / C++ (`x86_64`, `aarch64`, `riscv64`, `bpf`)**: `clang -O3 -flto=full -fuse-ld=lld` / `clang++ -std=c++23 -O3 -flto=full`.
- **Assembly (`core/asm/fairwind_kernel.S`)**: Dual-ABI (`_WIN32` x64 `rcx`/`rdx` + SysV `rdi`/`rsi`), links directly with C/C++/Rust LTO objects.
- **WebAssembly (`core/wasm/fairwind_wasm.c`)**: `clang --target=wasm32 -O3 -nostdlib -Wl,--no-entry -Wl,--export-all -Wl,--strip-all`, verified live via `WebAssembly.instantiate(...)`.
- **CUDA / NVPTX64 (`core/cuda/fairwind_kernel.cu`, `fairwind_nvptx.c`)**: LLVM `clang --target=nvptx64-nvidia-cuda -S -O3` -> PTX (`sm_80`/`sm_89`/`sm_90`) or `nvcc -O3 -dlto -arch=sm_89`.
- **WebGPU / WGSL (`core/webgpu/fairwind_compute.wgsl`)**: Compute shaders (`@compute @workgroup_size(256)`, `workgroupBarrier()`, `fma`, `inverseSqrt`).
- **Bun AOT (`src/fairwind_cli.ts`)**: `bun build --compile --aot --trim --no-compile-function-source`.
- **Go (`core/go/fairwind-go`)**: `go build -trimpath -ldflags="-s -w"` (and `-buildmode=c-shared` for C ABI FFI).
- **Java / Kotlin (`core/java`, `core/kotlin`)**: `javac --release 25` (Ghidra headless bridge) + Kotlin SDK contract.

## 2. Verification Command

Run `fairwind lowlevel verify` (or `bun run lowlevel:verify`) to compile and execute C+Asm, WebAssembly (`wasm32`), CUDA/NVPTX64 PTX, C++, WGSL, Go, and Java targets end-to-end.
