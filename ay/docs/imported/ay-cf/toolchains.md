# Polyglot Toolchains in `ay-cf`

| Toolchain | Config | Entrypoint / Verification |
| :--- | :--- | :--- |
| `just` (`1.59.0+`) | `justfile` | `just check`, `just finops`, `just reconcile`, `just cluster-proxy` |
| **Bun** (`1.4.5+`) | `package.json`, `bunfig.toml`, `patches/jsonc-parser@3.3.1.patch` | `just bun-check && just cf-audit` (`hono`, `cloudflare`, `zod`, `wrangler`) |
| **Rust** (`1.100.0`) | `rust-toolchain.toml`, `Cargo.toml`, `rustfmt.toml`, `clippy.toml` | `just rust-check` (`ay-cf-assets`, `ay-cf-edge`, `[patch.crates-io]` submodule hooks) |
| **C++23 / Bazel 9 / LLVM 23** | `.bazelversion`, `MODULE.bazel`, `.bazelrc`, `.clang-format` | `just cpp-check` (`abseil-cpp` CRC32C/StatusOr + `googletest`) |
| **Python 3.13 (`uv` + `ruff`)** | `.python-version`, `pyproject.toml`, `uv.lock` | `just py-check` (`cloudflare`, `pydantic` v2, `httpx`, `ruff`, `pytest`) |
