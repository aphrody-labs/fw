---
name: low-level-lto-aot
description: "Build and verify low-level Fat-LTO and AOT binaries across Assembly (x86-64/ARM64/RISC-V), WebAssembly (wasm32/WASI), CUDA/NVPTX64, WebGPU/WGSL, Rust (MSVC/Linux musl/Asterinas OSDK), C, C++, Go"
---
> , Java, Kotlin, Python, and Bun for every architecture inside Fw.


# Fw Low-Level Cross-Language & Cross-Architecture LTO/AOT Guide

Use `fw lowlevel matrix` and `fw lowlevel verify` to inspect and verify real native/GPU/Wasm builds across every supported architecture.

## 1. Compiler & Flag Matrix

- **Rust Static Zero-Dep (`x86_64-pc-windows-msvc`, `x86_64-unknown-linux-musl`, `aarch64`, `riscv64`)**: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`, `+crt-static` (enforced with `cargo clippy --workspace --all-targets -- -D warnings`).
- **Browser Wasm (aphrody `[profile.web]`: `opt-level = "z"`, thin LTO, `codegen-units = 1`, `panic = "abort"`)**: `cargo build` -> `wasm-bindgen 0.2.130` -> `wasm-opt 133 -Oz`; pins and the measured flag sets are in the `wasm-bindgen` skill (`toolchain.toml` group `wasm`).
- **Rust WebAssembly (`crates/fw-wasm`)**: `cargo build --release --target wasm32-unknown-unknown -p fw-wasm` (~872 B), verified live via `WebAssembly.instantiate(...)`.
- **Asterinas OSDK Framekernel (`os/fw-os`)**: `nightly-2026-11-16`, `#![no_std]`, `#![deny(unsafe_code)]`, `#[ostd::main]`, `OSDK.toml` (`grub-rescue-iso`, `microvm`, `virtio-gpu`, `tdx`).
- **C / C++ (`x86_64`, `aarch64`, `riscv64`, `bpf`)**: `clang -O3 -flto=full -fuse-ld=lld` / `clang++ -std=c++23 -O3 -flto=full`.
- **Assembly (`core/asm/fw_kernel.S`)**: Dual-ABI (`_WIN32` x64 `rcx`/`rdx` + SysV `rdi`/`rsi`), links directly with C/C++/Rust LTO objects.
- **C WebAssembly (`core/wasm/fw_wasm.c`)**: `clang --target=wasm32 -O3 -nostdlib -Wl,--no-entry -Wl,--export-all -Wl,--strip-all`.
- **CUDA / NVPTX64 (`core/cuda/fw_kernel.cu`, `fw_nvptx.c`)**: LLVM `clang --target=nvptx64-nvidia-cuda -S -O3` -> PTX (`sm_80`/`sm_89`/`sm_90`) or `nvcc -O3 -dlto -arch=sm_89`.
- **WebGPU / WGSL (`core/webgpu/fw_compute.wgsl`, `crates/fw-ostd-wasm`)**: Compute & GPUI quad shaders (`@compute`, `@vertex`, `@fragment`).
- **Bun AOT (`src/fw_cli.ts`)**: `bun build --compile --aot --trim --no-compile-function-source`.
- **Go (`core/go/fw-go`)**: `go build -trimpath -ldflags="-s -w"` (and `-buildmode=c-shared` for C ABI FFI).
- **Java / Kotlin (`core/java`, `kotlin`)**: `javac --release 25` (Ghidra headless bridge) + Kotlin SDK contract.

## 2. Verification Commands

- `fw lowlevel verify` — compiles & executes C+Asm, Wasm32, CUDA PTX, C++, WGSL, Go, and Java live.
- `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` — zero-warning Rust workspace gate.
- `cargo check --target x86_64-unknown-linux-musl -p fw-bin` — zero-dep Linux 7.3 static binary check.
