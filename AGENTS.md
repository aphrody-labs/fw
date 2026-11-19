# fw (fairwind) — Agent Contract (v4.2, Bun-in-Rust Methodology)

- **Read First (< 600 tokens)**: `MAP.tsv` (byte-exact directory map + gates), `LIFETIMES_AND_STYLE.md` (zero-abstraction Rust rules), `.agents/workflows/bun-in-rust-loop.md` (split-context adversarial review), `docs/architecture/os-strategy-2026.md` (long-term OS decision).
- **Token Hygiene**: `.rgignore` & `.fdignore` automatically hide vendored trees (`core/ts/neon`, `core/rs/craft-*`, `packages/`, `target/`, `uv.lock`) from default searches (`--no-ignore` only when auditing vendored trees).
- **Root 13-crate Rust Workspace + `xtask` (`Cargo.toml`)**: `crates/fw-{ggml,compute,cluster,proto,ui-blitz,wasm,ostd-wasm,uutils,nix,unified-stack,wayland-wgpu,hermes-claw,bin}` + `xtask` — `12/13 #![forbid(unsafe_code)]`, 5 external deps, zero Clippy warnings, zero stubs, zero skipped tests.
- **OS Architecture (2-Tier)**:
  - **Tier 1 Primary Host (Linux perf)**: NixOS `flake.nix` + `nix/` (`nix-cachyos-kernel` LTO `x86_64-v3`/`zen4`, `sched_ext` `scx_lavd`, `io_uring`, `uutils`, `sudo-rs`, `ntpd-rs`) + `os/fairwind-wsl` (custom BORE/`sched_ext`/ThinLTO/virtiofs/dxgkrnl WSL kernel).
  - **Tier 2 Sandbox MicroVM**: `cloud-hypervisor` / `openvmm` + optional `os/fairwind-os` (Asterinas OSDK `v0.19.0` guest R&D only).
- **Edge & Plugins**: `cloudflare/` (Workers Static Assets + SQLite DO), `web/`, `configs/heretic/`, `plugin/` (11 skills).
- **Upstream Read-Only Boundary**: `%LOCALAPPDATA%\agy\bin\agy.exe` is strictly read-only.
- **Gates (`just ci` / `cargo xtask ci`)**:
  - `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  - `cargo build --release -p fw-bin --bin fw && cargo build --release --target wasm32-unknown-unknown -p fw-wasm && cargo check --target x86_64-unknown-linux-musl -p fw-bin`
  - `uvx ruff check . && bun test && bun run cf:dry-run`
  - `cargo xtask purity && cargo xtask map` (keeps `MAP.tsv` byte-exact)
- **Git Discipline**: Commit explicit file paths on `main`; never `git stash` or `git reset --hard`; zero AI attribution trailers; keep repo private (`aphrody-labs/fairwind`).
