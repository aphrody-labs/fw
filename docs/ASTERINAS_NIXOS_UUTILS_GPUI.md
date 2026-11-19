# Fw v4.0 - 100% Pure-Rust OS (Asterinas OSDK + GPUI WebGPU/Wasm + Alpine `uutils` + NixOS)

## Architecture

Fw v4.0 unifies a safe-Rust Framekernel OS with an Alpine-lighter pure-Rust `uutils` multicall userland, pure-Rust NixOS store/NAR compatibility, Zed GPUI WebAssembly/WebGPU scene graph core, and all 23 Rust/Cloudflare/AI stacks:

- **Ring 0 - Asterinas OSDK Kernel** ([os/fw-os](file:///C:/src/fw/os/fw-os/OSDK.toml)): `#![no_std]`, `#![deny(unsafe_code)]`, `#[ostd::main]`, `nightly-2026-11-16`, `grub-rescue-iso`, `microvm`, `virtio-gpu`, and `tdx` schemes; upstream fork at `aphrody-labs/asterinas-1` (`packages/asterinas/`).
- **Ring 0/3 Bridge - `fw-ostd-wasm`** ([crates/fw-ostd-wasm/src/lib.rs](file:///C:/src/fw/crates/fw-ostd-wasm/src/lib.rs)): Portable Asterinas `XArray` + `MlsDisk` + **Zed GPUI** retained quad scene graph + WGSL shader tuned for WebAssembly, WebGPU (`gpui_wgpu`), and Cloudflare Workers.
- **Ring 3 Userland - `fw-uutils`** ([crates/fw-uutils/src/lib.rs](file:///C:/src/fw/crates/fw-uutils/src/lib.rs)) & [os/fw-os/APKBUILD](file:///C:/src/fw/os/fw-os/APKBUILD): Alpine `edge/community/uutils-coreutils`-compatible `argv[0]` multicall binary (< 1 MB Fat-LTO static `musl`, lighter than Alpine Linux's ~7.8 MB minirootfs, zero C BusyBox/GNU/glibc) + Wasm32 playground.
- **NixOS Compatibility - `fw-nix`** ([crates/fw-nix/src/lib.rs](file:///C:/src/fw/crates/fw-nix/src/lib.rs)) & [os/fw-os/flake.nix](file:///C:/src/fw/os/fw-os/flake.nix): Pure-Rust `nixbase32` `/nix/store/<hash>-<name>` addressing, NAR (Nix ARchive) binary encoder, and `nixosModules.fw` (`uutils-coreutils-noprefix`).
- **23 Unified Stacks - `fw-unified-stack`** ([crates/fw-unified-stack/src/lib.rs](file:///C:/src/fw/crates/fw-unified-stack/src/lib.rs)): Burn CubeCL, llama.cpp/GGUF v3/`ggml-rpc`, NVIDIA Dynamo, Rig, `rmcp`, ACP `rust-sdk`, `a2a-rs`, Cloudflare Pingora/Quiche/Foundations/`workers-rs`/`workerd`/`cloudflared`/`agents`/`cloudflare-os`/`mcp`/`kitesurf`, Yazi, Kitty Graphics/CSI-u/OSC 133, AnomalyCo `terminal-control` & `hex`.
