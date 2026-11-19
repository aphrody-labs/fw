#![no_std]
#![deny(unsafe_code)]

use ostd::prelude::*;

#[ostd::main]
fn kernel_main() {
    println!("Fw v4.0 Asterinas Framekernel booting (100% safe Rust, WebAssembly + WebGPU tuned)");
    println!("  - Userland: uutils pure-Rust multicall (Alpine-lighter, zero glibc/BusyBox)");
    println!("  - Store:    NixOS-compatible /nix/store + NAR + atomic generations");
    println!("  - Compute:  VirtIO-WebGPU / WGSL + Burn CubeCL + ggml-rpc + Wasm32/WASI-P2");
}
