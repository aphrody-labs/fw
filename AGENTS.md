# fairwind — Agent Contract

- Layout: `core/py/heretic`, `core/ts/neon`, `core/rs/craft-libs`, `core/rs/craft-anything`, `native/fairwind-abi`, `src/fairwind_cli.ts`, `tests/`. `packages/ghidra` is external.
- Gates before commit/push:
  - `uvx ruff check .`
  - `cargo test --manifest-path native/fairwind-abi/Cargo.toml`
  - `cargo build --release --manifest-path native/fairwind-abi/Cargo.toml`
  - `bun test tests`
  - `bun build --compile --bytecode src/fairwind_cli.ts --outfile dist/fairwind.exe`
  - `dist/fairwind.exe status`
- Work on `main`; zero AI attribution trailers; keep repository private (`aphrody-labs/fairwind`).
