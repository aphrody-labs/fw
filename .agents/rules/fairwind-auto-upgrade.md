---
trigger: always_on
description: Configures Antigravity (agy) inside Fairwind with target/agy LTO/AOT binaries covering all in-repo modules and deterministic self-upgrade gates.
---

# Fairwind Target & Autonomous Self-Upgrade Contract

- **Upstream boundary**: Never overwrite, patch, or tamper with Google's Authenticode-signed upstream `%LOCALAPPDATA%\agy\bin\agy.exe`. Treat it strictly as a read-only upstream binary inspected via `fairwind inspect` (`craft-pe` Fat-LTO ABI) and transparently delegated to by `target/agy/agy.exe`.
- **In-repo target directory**: `target/agy/` holds the verified LTO/AOT artifacts built from this monorepo:
  - `target/agy/agy.exe` — Rust Fat-LTO AOT native front-end & launcher (`native/fairwind-agy`)
  - `target/agy/fairwind.exe` — Bun AOT + `--trim` unified CLI (`src/fairwind_cli.ts`)
  - `target/agy/fairwind_abi.dll` — Rust Fat-LTO C ABI bridge (`native/fairwind-abi`, linking `core/rs/craft-libs/crates/pe` + 9 Tree-sitter grammars)
- **Tracked module registry**: `fairwind.targets.json` registers every in-repo module (`native/fairwind-pe`, `native/fairwind-abi`, `native/fairwind-agy`, `src`, `tests`, `core/py/heretic`, `core/ts/neon`, `core/rs/craft-libs`, `core/rs/craft-anything`, external `packages/ghidra`).
- **Self-upgrade workflow** (run whenever any registered in-repo code changes):
  1. `bun run upgrade:check` — detect module hash drift, missing artifacts, or upstream `agy.exe` changes against `target/manifest.json`.
  2. `bun run upgrade` (or `target/agy/agy.exe --fairwind-upgrade --apply`) — rebuild Fat-LTO Rust ABI + Fat-LTO `agy.exe` + Bun AOT `--trim` `fairwind.exe`, run all verification gates (`uvx ruff check .`, `cargo test`, `bun test tests`, staged smoke checks), atomically promote to `target/agy/` and `dist/` with `.prev` rollback safety, and record the verified build in `target/manifest.json` + `target/upgrade-history.jsonl`.
  3. `bun run bench` — verify binary size and cold-start latency.
