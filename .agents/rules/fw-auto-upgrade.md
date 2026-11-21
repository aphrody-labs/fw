---
trigger: always_on
description: Build and upgrade contract for the in-repo agy launcher, C ABI and plugin CLI under target/agy.
---

# Target and upgrade contract

- **Upstream boundary**: never overwrite or patch Google's signed `%LOCALAPPDATA%\agy\bin\agy.exe`. Inspect it read-only (`fw-plugin inspect`, which uses the `fw-abi` PE reader) and let `target/agy/agy.exe` delegate to it.
- **Artifacts** (paths and build profiles in `fw.targets.json`):
  - `target/agy/agy.exe`: Rust launcher built with `lto = "fat"` (`native/fw-agy`).
  - `target/agy/fw-plugin.exe`: the Bun CLI compiled with `--aot --trim` (`src/fw_cli.ts`).
  - `target/agy/fw_abi.dll`: Rust C ABI cdylib (`native/fw-abi`, `craft-pe` and `craft-sniff` from `aphrody-labs/craft-libs`, 12 Tree-sitter grammars).
- **Module registry**: `fw.targets.json` lists every module the upgrade tracks.
- **Upgrade** (run after any registered module changes):
  1. `bun run upgrade:check` reports hash drift, missing artifacts and a changed upstream `agy.exe` against `target/manifest.json`.
  2. `bun run upgrade` (or `target/agy/agy.exe --fw-upgrade --apply`) rebuilds the ABI, the launcher and the CLI, runs `uvx ruff check .` and the `native/fw-abi` and `native/fw-agy` tests, smoke-checks the staged binaries, promotes them to `target/agy/` and `dist/` keeping `.prev`, and records `target/manifest.json` and `target/upgrade-history.jsonl`.
  3. `bun run bench` measures binary size and cold-start latency.
