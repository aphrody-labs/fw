# OS components: Asterinas, uutils, Nix, GPUI

`os/fw-os` and the crates below are research components. The primary host OS decision is in `architecture/os-strategy-2026.md`.

- **Kernel**: `os/fw-os` ([OSDK.toml](../os/fw-os/OSDK.toml)) is a `#![no_std]`, `#![deny(unsafe_code)]` Asterinas OSDK kernel on `nightly-2026-11-16` (`os/fw-os/rust-toolchain.toml`); `os/fw-os/Cargo.toml` expects the Asterinas fork `aphrody-labs/asterinas-1` at `packages/asterinas/`, which is not checked out here.
- **`fw-ostd-wasm`** ([lib.rs](../crates/fw-ostd-wasm/src/lib.rs)): Asterinas `XArray` and `MlsDisk` plus a GPUI-style retained quad scene graph and a WGSL shader, for WebAssembly and Cloudflare Workers.
- **`fw-uutils`** ([lib.rs](../crates/fw-uutils/src/lib.rs), [APKBUILD](../os/fw-os/APKBUILD)): `argv[0]` multicall binary of the applets in `SUPPORTED_APPLETS`, with a Wasm32 playground.
- **`fw-nix`** ([lib.rs](../crates/fw-nix/src/lib.rs), [flake.nix](../os/fw-os/flake.nix)): `nixbase32` `/nix/store/<hash>-<name>` paths, a NAR encoder and `nixosModules.fw`.
- **`fw-unified-stack`** ([lib.rs](../crates/fw-unified-stack/src/lib.rs)): descriptors for the Rust stacks fw tracks; it links none of them.
