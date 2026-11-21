---
name: fw
description: Orientation for the fw monorepo: the Rust workspace under `crates/fw-*` and `xtask`, the `fw` binary, the Bun plugin CLI, the native agy launcher and C ABI, and the Cloudflare and Vercel adapters.
---

# fw

## Layout

- **Rust workspace** (`Cargo.toml`): `crates/fw-*` and `xtask`. List members with `cargo metadata --no-deps --format-version 1`; tiers and gates per directory are in `MAP.tsv`.
- **`fw` binary** (`crates/fw-bin`): `fw --help` lists commands. It also keeps older subcommands: `status | hardware | compute | ggml | cluster [route] | openapi | ui [--out] | ostd | uutils <applet> | nix <name> | stack | wayland | claw | serve [addr]`, and `uutils` `argv[0]` multicall dispatch.
- **Automation** (`xtask/`): `cargo xtask ci [fast|all]`, `purity`, `map`, `package`, `doctor`, `sync-upstreams`, `ship`.
- **WebAssembly**: `target/wasm32-unknown-unknown/release/fw_wasm.wasm` (`crates/fw-wasm`), used by the Cloudflare Worker and the browser UI.
- **OS work** (`docs/architecture/os-strategy-2026.md`): NixOS `flake.nix` and `nix/` (CachyOS LTO kernel, `scx_lavd`, `uutils-coreutils-noprefix`, `sudo-rs`, `ntpd-rs`), `os/fw-wsl` (WSL kernel), `cloud-hypervisor` MicroVM sandbox, and `os/fw-os` (Asterinas OSDK guest, research only).

## Native and Bun targets

- `native/fw-agy`: `target/agy/agy.exe` launcher; the signed Google `agy.exe` stays read-only.
- `src/fw_cli.ts`: Bun CLI compiled to `dist/fw-plugin.exe` and `target/agy/fw-plugin.exe` (`bun build --compile --aot --trim`).
- `native/fw-abi`: C ABI cdylib (`target/agy/fw_abi.dll`): 12 Tree-sitter grammars, `craft-pe`, `craft-sniff`, `iced-x86`, hashing, SSH, Git, OCI, GGUF/GGML. On Windows `src/targets.ts` shadow-copies the DLL to `%TEMP%\fw-abi-shadow\` before loading, so `cargo build --release` does not hit file locks.
- Decisions: `fw decide`, `POST /v1beta/decisions` (`src/decisions.ts`).
- Terminal: `fw tui status|wta-config|verify-self|verify`, MCP tool `fw_tui_terminal`.
- Cloud: `cloudflare/worker.ts` (`FwOSAgent` Durable Object on SQLite) and `vercel/api/fw.ts`.

## Gates

1. `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
2. `cargo build --release -p fw-bin && cargo build --release --target wasm32-unknown-unknown -p fw-wasm && cargo check --target x86_64-unknown-linux-musl -p fw-bin`
3. `bun test` (`bunfig.toml` limits discovery to `./tests`)
4. `uvx ruff check . && uv run python -m unittest discover -s tests`
5. `bun run upgrade`, then `dist/fw-plugin.exe plugin sync --install`, `dedupe` and `target verify`.
6. Windows PowerShell 5.1: pipe scripts through `@' ... '@ | bun -` instead of `bun -e "..."`, and use `gh api ... | ConvertFrom-Json` instead of `--jq` with backslashes.
