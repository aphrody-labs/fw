---
name: rio-terminal
description: "Rio GPU Terminal (v0.6.0, aphrody-labs/rio) architecture, Sugarloaf WebGPU/Wasm renderer, rio-vt, librio C ABI pull model, librio-wasm, rio-ai (ACP/OSC 7501/Session MCP), Native Windows ConPTY/DX12"
---
> , and Cloudflare Edge high support.


# Rio GPU Terminal (`aphrody-labs/rio`) Skill

Verified live against `aphrody-labs/rio` (`0.6.0`, synced with `raphamorim/rio` `2026-11-19`).

## 1. Workspace Crate Architecture

- **`sugarloaf` / `libsugarloaf`**: Pure-Rust WebGPU (`wgpu`) + WebAssembly (`wasm32`) GPU text/glyph/quad renderer (DX12 on Windows, Metal on macOS, Vulkan on Linux, WebGPU/WebGL2 in browser).
- **`rio-vt`**: Published, idiomatic Rust VT state machine (`Machine`, `performer`) and cell grid (`Crosswords`). Depend on `rio-vt` directly in Rust projects.
- **`librio`**: Embeddable C ABI wrapper over `rio-vt` + `teletypewriter` (PTY + VT only, zero windowing stack). Pull-based `RenderState` with per-row dirty flags and minimal callback surface (`wakeup_cb`, `action_cb`, `clipboard_write`, `close_surface`, and `program_status_cb` for **OSC 7501**).
- **`librio-wasm` / `frontends/wasm`**: Browser WebAssembly + WebGPU target suitable for Cloudflare Workers Static Assets deployment.
- **`teletypewriter` + `corcovado`**: Cross-platform PTY (ConPTY on Windows, POSIX PTY on Unix) and async I/O event loop.
- **`rio-ai`** (`aphrody-labs/rio` extension): `#![forbid(unsafe_code)]` AI orchestrator adding Microsoft Intelligent Terminal ACP 1.3.0 agent registry, OSC 7501 status codec, OSC 133 autofix queueing, per-tab stashed/visible agent panes, Session MCP 4 tools, Native Windows NamedPipe (`\\.\pipe\rio-ai-master-<id>`) + SDDL (`D:(A;;GA;;;OW)`), and Cloudflare Workers AI / AI Gateway / Zero Trust edge routing.

## 2. Build & Verification Commands

- `cargo clippy --manifest-path C:\src\rio\Cargo.toml -p rio-ai --all-targets -- -D warnings`
- `cargo test --manifest-path C:\src\rio\Cargo.toml -p rio-ai`
- `cargo build --release --manifest-path C:\src\rio\Cargo.toml -p rioterm` (native GPU terminal)
- `cargo build --release --target wasm32-unknown-unknown --manifest-path C:\src\rio\Cargo.toml -p librio-wasm` (WebGPU/Wasm edge build)
