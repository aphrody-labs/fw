# fairwind

Unified product and Antigravity plugin absorbing `heretic` (`core/py/heretic`), `neon`/REA (`core/ts/neon`), `craft-libs` (`core/rs/craft-libs`), and `craft-anything` (`core/rs/craft-anything`) alongside `agy-knowledge` and autonomous `learn` behind:

- `target/agy/agy.exe` (`native/fairwind-agy`) — Rust Fat-LTO AOT native `agy` front-end, PE inspector, and self-upgrade launcher (delegates transparently to the read-only signed upstream `agy.exe` and `agy-studio.exe`)
- `dist/fairwind.exe` & `target/agy/fairwind.exe` (`src/fairwind_cli.ts`) — Bun AOT + `--trim` unified CLI, MCP server (`fairwind mcp`), HTTP JSON API (`fairwind api`), and continuous-upgrade daemon (`fairwind daemon` / `fairwind service`)
- `fairwind_abi.dll` (`native/fairwind-abi`) — Rust Fat-LTO C ABI v3 bridge linking `core/rs/craft-libs/crates/pe` (`craft-pe` via `native/fairwind-pe`) and 9 Tree-sitter grammars
- `plugin/` — strict-schema Antigravity plugin (`fairwind`) unifying `mcp_config.json`, `rules/AGENTS.md`, and skills (`fairwind`, `agy-knowledge`, `learn`)

`packages/ghidra` remains external. All in-repo code modules are registered in `fairwind.targets.json` and tracked in `target/manifest.json` for deterministic self-upgrade.

## Commands

- `bun run check` — `uvx ruff check .`, `cargo test` (`fairwind-pe`, `fairwind-abi`, `fairwind-agy`), `bun test tests`
- `bun run build` — release Fat-LTO C ABI DLL, Fat-LTO `agy` launcher, and Bun AOT `--trim` `dist/fairwind.exe`
- `bun run target:scan` — scan all in-repo modules, probe Tree-sitter ASTs, inspect PE targets, and update `target/manifest.json`
- `bun run upgrade:check` — detect drift across all in-repo code and upstream `agy.exe`
- `bun run upgrade` — rebuild, gate-verify, and atomically promote binaries to `target/agy/` and `dist/` with rollback safety
- `bun run plugin:sync` — mirror `plugin/` to `~/.gemini/plugins/fairwind` & `~/.gemini/config/plugins/fairwind`, run `install.ts`, and validate with `agy plugin validate --strict`
- `bun run service:render` / `bun run service:install` / `bun run service:status` — render, install, and inspect the persistent boot-startup auto-restart daemon (Windows Task Scheduler, Linux systemd `--user`, macOS launchd)
- `bun run daemon:once` — run one deterministic upgrade + learning + plugin sync cycle
- `bun run mcp` / `bun run api` — launch unified stdio MCP server or local `127.0.0.1:4379` JSON API
- `bun run bench` — compare binary size and cold-start latency across `target/agy/agy.exe`, `dist/fairwind.exe`, and upstream `agy.exe`
- `bun run status` — run `dist/fairwind.exe status`
