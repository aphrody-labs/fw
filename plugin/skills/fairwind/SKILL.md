---
name: fairwind
description: Unified Fairwind v4.0 monorepo, plugin, 12-crate zero-dependency Rust OS workspace (fairwind-native, fw-wasm, Asterinas OSDK, GPUI WebGPU/Wasm, Wayland-WGPU, Alpine uutils, NixOS store/NAR, ggml-org, Pingora+Dynamo+vLLM-Router), Fat-LTO Rust C ABI v4, Bun AOT CLI, MCP server, HTTP/WS/WebRTC/GraphQL API, Gemini Decision 1, Cloudflare/Vercel, Intelligent Terminal (WTA) + tui-test, and persistent continuous agy upgrade daemon.
---

# Fairwind v4.0 Unified OS, Cluster & Continuous `agy` Upgrade Stack

## 1. Zero-Dependency Rust v4.0 Workspace (`Cargo.toml` — 12 Crates, 0 Clippy Warnings)

- **Unified static binary**: `target/release/fairwind-native.exe` (`crates/fw-bin`, ~637 KiB Fat-LTO zero-dep; cross-checked for `x86_64-unknown-linux-musl` static). Subcommands: `status | hardware | compute | ggml | cluster [route] | openapi | ui [--out] | ostd | uutils <applet> | nix <name> | stack | wayland | serve [addr]` + Alpine `uutils` `argv[0]` multicall dispatch.
- **WebAssembly module**: `target/wasm32-unknown-unknown/release/fw_wasm.wasm` (`crates/fw-wasm`, ~872 B) for Cloudflare Workers & End-User Browser WebGPU/FPU compute.
- **Asterinas OSDK Kernel + NixOS + Alpine**: `os/fairwind-os/` (`OSDK.toml` with `grub-rescue-iso`/`microvm`/`virtio-gpu`/`tdx`, `rust-toolchain.toml` `nightly-2026-11-16`, `#![no_std] #![deny(unsafe_code)] #[ostd::main]`, `flake.nix` with `uutils-coreutils-noprefix`, and `APKBUILD`).
- **Core `#![forbid(unsafe_code)]` crates**: `fw-ostd-wasm` (Asterinas `XArray`/`MlsDisk` + Zed GPUI retained quad scene + WGSL), `fw-wayland-wgpu` (zero-C-dep Wayland wire protocol + WebGPU/GPUI compositor), `fw-uutils` (pure-Rust Alpine-lighter multicall + Wasm32 playground), `fw-nix` (`nixbase32` `/nix/store` + NAR binary encoder), `fw-ggml` (`introduction-to-ggml` 4D tensor/cgraph + GGUF v3 + `ggml-rpc`), `fw-compute` (CUDA `sm_89`/WebGPU/Vulkan/Metal/CPU/TPU PJRT), `fw-cluster` (Pingora + `ai-dynamo` + `vllm-router` KV-cache P/D router across 4 GPU + 3 VPS/CPU + End-User WebGPU), `fw-proto` (HTTP/WS/WebRTC/GraphQL/OpenAPI 3.1/OpenAI/MCP/ACP/A2A/`/webgpu`), `fw-ui-blitz` (DioxusLabs/blitz zero-dep PNG renderer), `fw-unified-stack` (Burn CubeCL, Rig, `rmcp`, ACP, `a2a-rs`, Quiche, Foundations, Kitesurf, Yazi, Kitty Graphics/CSI-u/OSC 133, AnomalyCo `terminal-control` & `hex`).

## 2. Bun AOT CLI, Fat-LTO C ABI v4 & `agy` Target Pipeline

- **Native Fat-LTO `agy` launcher**: `target/agy/agy.exe` (`native/fairwind-agy`, ~381 KiB vs 235.4 MB signed read-only Google upstream `agy.exe`).
- **Bun AOT `--trim` CLI**: `dist/fairwind.exe` & `target/agy/fairwind.exe` (`bun build --compile --aot --trim --no-compile-function-source src/fairwind_cli.ts`).
- **Fat-LTO Rust C ABI v4**: `target/agy/fairwind_abi.dll` (`native/fairwind-abi`, 11 Tree-sitter 0.27 grammars, `craft-pe`, `craft-sniff`, `iced-x86`, SHA-256/512, BLAKE3, SSH, Git, OCI, GGUF/GGML). On Windows, `src/targets.ts` shadow-copies the DLL to `%TEMP%\fairwind-abi-shadow\` before `dlopen` to prevent `os error 5` locks during `cargo build --release`.
- **Gemini Decision 1 & Self-Hosted AI**: `fairwind decide`, `fairwind workflow`, `POST /v1beta/decisions` (`src/decisions.ts`, `noul`/`choice`/`score`, 0 output tokens).
- **Intelligent Terminal (WTA) & `tui-test`**: source-only `core/rs/wta` + `core/rs/tui-test` exposed via `fairwind tui status|wta-config|verify-self|verify` and MCP tool `fairwind_tui_terminal`.
- **Cross-Cloud**: `cloudflare/worker.ts` (`FairwindOSAgent` DO SQLite + 3-tier execution + Gatekeeper) and `vercel/api/fairwind.ts` (Bun 1.4.x Fluid Compute).

## 3. Verification & Hygiene Gates

1. `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
2. `cargo build --release -p fw-bin && cargo build --release --target wasm32-unknown-unknown -p fw-wasm && cargo check --target x86_64-unknown-linux-musl -p fw-bin`
3. `bun test` (root `bunfig.toml` scopes test discovery strictly to `./tests`)
4. `uvx ruff check . && uv run python -m unittest discover -s tests`
5. `bun run upgrade && .\dist\fairwind.exe plugin sync --install && .\dist\fairwind.exe dedupe && .\dist\fairwind.exe target verify`
6. On Windows PowerShell 5.1: use `@' ... '@ | bun -` heredocs instead of `bun -e "..."`, and `gh api ... | ConvertFrom-Json` instead of `--jq` with backslashes.
