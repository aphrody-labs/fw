# fairwind

Unified product absorbing `heretic` (`core/py/heretic`), `neon` (`core/ts/neon`), `craft-libs` (`core/rs/craft-libs`), and `craft-anything` (`core/rs/craft-anything`) behind one compiled CLI (`src/fairwind_cli.ts` -> `dist/fairwind.exe`) and Rust C ABI (`native/fairwind-abi`). `packages/ghidra` remains external.

## Commands

- `bun run check` — `uvx ruff check .`, `cargo test`, `bun test tests`
- `bun run build` — release C ABI DLL + compiled `dist/fairwind.exe`
- `bun run status` — run `dist/fairwind.exe status`
