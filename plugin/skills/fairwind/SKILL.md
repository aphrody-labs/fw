---
name: fairwind
description: Unified fw (Fairwind) v4.2 monorepo, plugin, 13-crate + xtask zero-bloat Rust workspace (fw, fw-wasm, fw-hermes-claw, GPUI WebGPU/Wasm, Wayland-WGPU, uutils, NixOS store/NAR, ggml, Pingora+Dynamo+vLLM-Router), 2-tier Linux CachyOS LTO + sched_ext primary OS & microVM sandbox (Cloud Hypervisor / OpenVMM / optional Asterinas OSDK guest), Fat-LTO Rust C ABI v4, Bun AOT CLI, Cloudflare/Vercel, and continuous agy upgrade pipeline.
---

# `fw` (Fairwind v4.2) Unified OS, Cluster & Continuous `agy` Upgrade Stack

## 1. Zero-Bloat Rust v4.2 Workspace (`Cargo.toml` — 13 Crates + `xtask`, 5 External Deps, 0 Warnings)

- **Unified static binary**: `target/release/fw.exe` (`crates/fw-bin`, Fat-LTO; cross-checked for `x86_64-unknown-linux-musl`). Subcommands: `status | hardware | compute | ggml | cluster [route] | openapi | ui [--out] | ostd | uutils <applet> | nix <name> | stack | wayland | claw | serve [addr]` + `uutils` `argv[0]` multicall dispatch.
- **Pure-Rust automation (`xtask/`)**: `cargo xtask ci [fast|full]`, `cargo xtask purity`, `cargo xtask map`, `cargo xtask sync-upstreams`.
- **WebAssembly module**: `target/wasm32-unknown-unknown/release/fw_wasm.wasm` (`crates/fw-wasm`) for Cloudflare Workers & Browser WebGPU/FPU compute.
- **2-Tier OS Architecture (`docs/architecture/os-strategy-2026.md`)**:
  - **Tier 1 Primary Host (Linux perf)**: NixOS `flake.nix` + `nix/` (`nix-cachyos-kernel` LTO `x86_64-v3`/`zen4`, `sched_ext` `scx_lavd`, `io_uring`, `uutils-coreutils-noprefix`, `sudo-rs`, `ntpd-rs`) + `os/fairwind-wsl` (custom BORE/`sched_ext`/ThinLTO/`virtiofs`/`dxgkrnl` WSL kernel).
  - **Tier 2 MicroVM Sandbox**: `cloud-hypervisor` / `openvmm` + optional `os/fairwind-os` (Asterinas OSDK `v0.19.0` guest R&D only, non-primary).
- **Core `#![forbid(unsafe_code)]` crates (12/13)**: `fw-hermes-claw` (Hermes memory/distiller + OpenClaw/ZeroClaw gateway + CLI-to-MCP + CLI-to-Skill), `fw-ostd-wasm`, `fw-wayland-wgpu`, `fw-uutils`, `fw-nix`, `fw-ggml`, `fw-compute`, `fw-cluster`, `fw-proto`, `fw-ui-blitz`, `fw-unified-stack`, `fw-bin`.

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
