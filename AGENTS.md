# fairwind — Agent Contract

- Layout: `core/py/heretic`, `core/ts/neon`, `core/rs/craft-libs`, `core/rs/craft-anything`, `native/fairwind-pe`, `native/fairwind-abi`, `native/fairwind-agy`, `plugin/`, `src/`, `fairwind.targets.json`, `.agents/rules/fairwind-auto-upgrade.md`, `tests/`. `packages/ghidra` is external.
- Unified plugin (`plugin/` -> `~/.gemini/plugins/fairwind` & `~/.gemini/config/plugins/fairwind`): bundles unified MCP (`fairwind mcp`), `rules/AGENTS.md`, and skills `fairwind`, `agy-knowledge`, and `learn`.
- Target & self-upgrade layout: `target/agy/` (`agy.exe` Rust Fat-LTO AOT launcher, `fairwind.exe` Bun AOT + `--trim` CLI, `fairwind_abi.dll` Rust Fat-LTO C ABI), tracked against all in-repo code in `target/manifest.json`, `target/upgrade-history.jsonl`, and `target/daemon-state.json`. Upstream Google-signed `%LOCALAPPDATA%\agy\bin\agy.exe` is strictly read-only.
- Persistent cross-platform daemon: `fairwind service render|install|status` (Windows Task Scheduler, Linux systemd `--user`, macOS launchd) running `fairwind daemon run --interval=300 --auto-apply --with-learn`.
- Gates before commit/push:
  - `uvx ruff check .`
  - `cargo test --manifest-path native/fairwind-pe/Cargo.toml`
  - `cargo test --manifest-path native/fairwind-abi/Cargo.toml`
  - `cargo test --manifest-path native/fairwind-agy/Cargo.toml`
  - `cargo build --release --manifest-path native/fairwind-abi/Cargo.toml`
  - `cargo build --release --manifest-path native/fairwind-agy/Cargo.toml`
  - `bun test tests`
  - `bun src/fairwind_cli.ts upgrade --apply --skip-tests`
  - `bun src/fairwind_cli.ts service render`
  - `dist/fairwind.exe status`
  - `target/agy/agy.exe --fairwind-info`
- Work on `main`; zero AI attribution trailers; keep repository private (`aphrody-labs/fairwind`).
