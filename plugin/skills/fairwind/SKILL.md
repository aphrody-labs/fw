---
name: fairwind
description: Unified Fairwind monorepo, plugin, CLI, Fat-LTO Rust FFI/C ABI, MCP server, HTTP API, absorbed subsystems (heretic, neon/REA, craft-libs/craft-pe, craft-anything, ghidra), and persistent cross-platform continuous agy upgrade daemon. Use for any Fairwind build, binary PE audit, Tree-sitter AST analysis, self-upgrade, or persistent agy improvement workflow.
---

# Fairwind Unified Stack & Continuous `agy` Upgrade Workflow

Fairwind fuses all first-party and third-party reverse-engineering, binary analysis, LLM tuning, autonomous learning, and `agy` acceleration tooling behind one reproducible pipeline.

## 1. Unified Surfaces (CLI, FFI, MCP, Lib, API)

- **Native Fat-LTO `agy` launcher**: `target/agy/agy.exe` (`native/fairwind-agy`, Rust Fat-LTO AOT, ~330 KB) — transparent direct-process delegation to read-only signed upstream `agy.exe` and `agy-studio.exe`, plus `--fairwind-info`, `--fairwind-inspect`, `--fairwind-target`, `--fairwind-upgrade`, `--fairwind-bench`, `fairwind <subcmd>`.
- **Unified Bun AOT CLI**: `dist/fairwind.exe` & `target/agy/fairwind.exe` (`bun build --compile --aot --trim --no-compile-function-source src/fairwind_cli.ts`).
- **Fat-LTO Rust C ABI v3**: `fairwind_abi.dll` / `libfairwind_abi.so` (`native/fairwind-abi`) — links `core/rs/craft-libs/crates/pe` (`craft-pe` via `native/fairwind-pe`), `sha2`, and 9 Tree-sitter grammars (`rust`, `typescript`, `tsx`, `python`, `c`, `cpp`, `java`, `json`, `bash`).
- **Unified MCP server**: `fairwind mcp` (exposed automatically by the `fairwind` plugin via `mcp_config.json`) — tools: `fairwind_status`, `fairwind_ast_parse`, `fairwind_pe_inspect`, `fairwind_target_scan`, `fairwind_upgrade`, `fairwind_bench`, `fairwind_learn`, `fairwind_service`, `fairwind_bridge`.
- **Unified Programmatic Library & HTTP API**: `src/index.ts` exports all modules; `fairwind api --port=4379` serves `/health`, `/status`, `/targets`, `/inspect`, `/ast`, `/upgrade/check`, `/bench`, `/learn/search`, `/daemon/status` on `127.0.0.1`.
- **Absorbed & External Subsystems**: `heretic` (`core/py/heretic`), `neon`/REA (`core/ts/neon`), `craft-libs` (`core/rs/craft-libs`), `craft-anything` (`core/rs/craft-anything`), `ghidra` (`packages/ghidra`).

## 2. Reproducible Target & Self-Upgrade Pipeline

1. **Inspect upstream `agy.exe` (read-only)**: `fairwind inspect` or `target/agy/agy.exe --fairwind-inspect`
2. **Scan all in-repo code & update `target/manifest.json`**: `bun run target:scan`
3. **Check drift across code, artifacts, and upstream `agy.exe`**: `bun run upgrade:check`
4. **Build, gate-verify, and atomically promote into `target/agy/` & `dist/`**: `bun run upgrade` (preserves `.prev` rollback backup and logs to `target/upgrade-history.jsonl`)
5. **Benchmark binary size & cold-start latency**: `bun run bench`

## 3. Persistent Boot-Startup Auto-Restart Cross-Platform Daemon

- **Single deterministic cycle**: `fairwind daemon run --once --auto-apply --with-learn --sync-plugin`
- **Render cross-platform manifests** (Windows Task Scheduler XML, Linux systemd user unit, macOS launchd plist into `target/service/`): `fairwind service render`
- **Install & verify active OS service**: `fairwind service install` -> `fairwind service status`
- **Sync & strictly validate plugin**: `fairwind plugin sync --install` (mirrors `plugin/` to `~/.gemini/plugins/fairwind` & `~/.gemini/config/plugins/fairwind`, runs `install.ts` and `agy plugin validate --strict --json`).
