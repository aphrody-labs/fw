---
trigger: always_on
description: Build and upgrade contract for the C ABI and plugin CLI under target/fw.
---

# Target and upgrade contract

- **Upstream boundary**: never overwrite or patch signed upstream binaries. Inspect them read-only (`fw-plugin inspect <pe-file>`, which uses the `fw-abi` PE reader).
- **Artifacts** (paths and build profiles in `fw.targets.json`):
  - `target/fw/fw-plugin.exe`: the Bun CLI compiled with `--aot --trim` (`src/fw_cli.ts`).
  - `target/fw/fw_abi.dll`: Rust C ABI cdylib (`native/fw-abi`, `craft-pe` and `craft-sniff` from `aphrody-labs/craft-libs`, 12 Tree-sitter grammars).
- **Module registry**: `fw.targets.json` lists every module the upgrade tracks.
- **Upgrade** (run after any registered module changes):
  1. `bun run upgrade:check` reports hash drift and missing artifacts against `target/manifest.json`.
  2. `bun run upgrade` rebuilds the ABI and the CLI, runs `uvx ruff check .` and the `native/fw-abi` tests, smoke-checks the staged binaries, promotes them to `target/fw/` and `dist/` keeping `.prev`, and records `target/manifest.json` and `target/upgrade-history.jsonl`.
  3. `bun run bench` measures binary size and cold-start latency.
