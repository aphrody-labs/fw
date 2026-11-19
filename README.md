# fw

Unified product and Antigravity plugin absorbing `heretic` (`core/py/heretic`), `neon`/REA (`core/ts/neon`), `craft-libs` (`core/rs/craft-libs`), and `craft-anything` (`core/rs/craft-anything`) alongside `agy-knowledge` and autonomous `learn` behind:

- `target/agy/agy.exe` (`native/fw-agy`) - Rust Fat-LTO AOT native `agy` front-end, PE inspector, and self-upgrade launcher (delegates transparently to the read-only signed upstream `agy.exe` and `agy-studio.exe`)
- `dist/fw.exe` & `target/agy/fw.exe` (`src/fw_cli.ts`) - Bun AOT + `--trim` unified CLI, MCP server (`fw mcp`), HTTP JSON API (`fw api`), and continuous-upgrade daemon (`fw daemon` / `fw service`)
- `fw_abi.dll` (`native/fw-abi`) - Rust Fat-LTO C ABI v3 bridge linking `core/rs/craft-libs/crates/pe` (`craft-pe` via `native/fw-pe`) and 9 Tree-sitter grammars
- `plugin/` - strict-schema Antigravity plugin (`fw`) unifying `mcp_config.json`, `rules/AGENTS.md`, and skills (`fw`, `agy-knowledge`, `learn`)

`packages/ghidra` remains external. All in-repo code modules are registered in `fw.targets.json` and tracked in `target/manifest.json` for deterministic self-upgrade.

## Commands

- `bun run check` - `uvx ruff check .`, `cargo test` (`fw-pe`, `fw-abi`, `fw-agy`), `bun test tests`
- `bun run build` - release Fat-LTO C ABI DLL, Fat-LTO `agy` launcher, and Bun AOT `--trim` `dist/fw.exe`
- `bun run target:scan` - scan all in-repo modules, probe Tree-sitter ASTs, inspect PE targets, and update `target/manifest.json`
- `bun run upgrade:check` - detect drift across all in-repo code and upstream `agy.exe`
- `bun run upgrade` - rebuild, gate-verify, and atomically promote binaries to `target/agy/` and `dist/` with rollback safety
- `bun run plugin:sync` - mirror `plugin/` to `~/.gemini/plugins/fw` & `~/.gemini/config/plugins/fw`, run `install.ts`, and validate with `agy plugin validate --strict`
- `bun run service:render` / `bun run service:install` / `bun run service:status` - render, install, and inspect the persistent boot-startup auto-restart daemon (Windows Task Scheduler, Linux systemd `--user`, macOS launchd)
- `bun run daemon:once` - run one deterministic upgrade + learning + plugin sync cycle
- `bun run mcp` / `bun run api` - launch unified stdio MCP server or local `127.0.0.1:4379` JSON API
- `bun run bench` - compare binary size and cold-start latency across `target/agy/agy.exe`, `dist/fw.exe`, and upstream `agy.exe`
- `bun run status` - run `dist/fw.exe status`
