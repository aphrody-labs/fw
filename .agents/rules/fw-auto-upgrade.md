---
trigger: always_on
description: Configures Antigravity (agy) inside Fw with target/agy LTO/AOT binaries covering all in-repo modules and deterministic self-upgrade gates.
---

# Fw Target & Autonomous Self-Upgrade Contract

- **Upstream boundary**: Never overwrite, patch, or tamper with Google's Authenticode-signed upstream `%LOCALAPPDATA%\agy\bin\agy.exe`. Treat it strictly as a read-only upstream binary inspected via `fw inspect` (`craft-pe` Fat-LTO ABI) and transparently delegated to by `target/agy/agy.exe`.
- **In-repo target directory**: `target/agy/` holds the verified LTO/AOT artifacts built from this monorepo:
  - `target/agy/agy.exe` — Rust Fat-LTO AOT native front-end & launcher (`native/fw-agy`)
  - `target/agy/fw-plugin.exe` — Bun AOT + `--trim` unified CLI (`src/fw_cli.ts`)
  - `target/agy/fw_abi.dll` — Rust Fat-LTO C ABI bridge (`native/fw-abi`, linking `core/rs/craft-libs/crates/pe` + 9 Tree-sitter grammars)
- **Tracked module registry**: `fw.targets.json` registers every in-repo module (`native/fw-kernel`, `native/fw-world`, `native/fw-abi`, `native/fw-agy`, `src`, `tests`, `plugin`, `crates`, `core/go/fw-go`, `cloudflare`, `vercel`).
- **Self-upgrade workflow** (run whenever any registered in-repo code changes):
  1. `bun run upgrade:check` — detect module hash drift, missing artifacts, or upstream `agy.exe` changes against `target/manifest.json`.
  2. `bun run upgrade` (or `target/agy/agy.exe --fw-upgrade --apply`) — rebuild Fat-LTO Rust ABI + Fat-LTO `agy.exe` + Bun AOT `--trim` `fw-plugin.exe`, run all verification gates (`uvx ruff check .`, `cargo test`, `bun test tests`, staged smoke checks), atomically promote to `target/agy/` and `dist/` with `.prev` rollback safety, and record the verified build in `target/manifest.json` + `target/upgrade-history.jsonl`.
  3. `bun run bench` — verify binary size and cold-start latency.
