# fairwind — Agent Contract (v4.2, Bun-in-Rust Methodology)

- **Read First (< 600 tokens)**: `MAP.tsv` (byte-exact directory map + gates), `LIFETIMES_AND_STYLE.md` (zero-abstraction Rust rules), `.agents/workflows/bun-in-rust-loop.md` (split-context adversarial review).
- **Token Hygiene**: `.rgignore` & `.fdignore` automatically hide vendored trees (`core/ts/neon`, `core/rs/craft-*`, `packages/`, `target/`, `uv.lock`) from default searches (`--no-ignore` only when auditing vendored trees).
- **Root 13-crate Rust Workspace (`Cargo.toml`)**: `crates/fw-{ggml,compute,cluster,proto,ui-blitz,wasm,ostd-wasm,uutils,nix,unified-stack,wayland-wgpu,hermes-claw,bin}` — `#![forbid(unsafe_code)]`, zero system deps, zero Clippy warnings, zero stubs, zero skipped tests.
- **OS, Fleet & Edge**: `os/{fairwind-os,fairwind-wsl}`, `nix/` + `flake.nix` (`nix-cachyos-kernel` + `rust-userland`), `cloudflare/` (Workers Static Assets + SQLite DO), `web/`, `configs/heretic/`, `plugin/` (10 skills).
- **Upstream Read-Only Boundary**: `%LOCALAPPDATA%\agy\bin\agy.exe` is strictly read-only.
- **Gates (`just ci`)**:
  - `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  - `cargo build --release -p fw-bin && cargo build --release --target wasm32-unknown-unknown -p fw-wasm && cargo check --target x86_64-unknown-linux-musl -p fw-bin`
  - `uvx ruff check . && bun test && bun run cf:dry-run`
  - `just map` (keeps `MAP.tsv` byte-exact)
- **Git Discipline**: Commit explicit file paths on `main`; never `git stash` or `git reset --hard`; zero AI attribution trailers; keep repo private (`aphrody-labs/fairwind`).
