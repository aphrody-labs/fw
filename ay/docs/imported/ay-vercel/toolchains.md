# Polyglot Toolchains in `ay-vercel`

| Toolchain | Config | Entrypoint / Verification |
| :--- | :--- | :--- |
| `just` (`1.59.0+`) | `justfile` | `just check`, `just turbo-build`, `just sync`, `just skills-sync`, `just mcp-sync`, `just docs-sync` |
| **Vercel TS + Native Stack** | `package.json`, `turbo.json`, `bunfig.toml` | `tsc`, `tsc-ts`, `scriptc`, `turbo`, `next`, `vercel`, `@vercel/sdk`, `ai`, `flags`, `workflow`, `chat`, `mcp-handler`, `@vercel/global-config`, `@vercel/blob`, `@vercel/sandbox`, `@vercel/otel` |
| **Rust** (`1.100.0+`) | `rust-toolchain.toml`, `Cargo.toml`, `rustfmt.toml`, `clippy.toml` | `just rust-check` (`ay-vercel-manifest`, `ay-vercel-skills`, `ay-vercel-function` on `vercel_runtime`) |
| **Python 3.13 (`uv` + `ruff`)** | `.python-version`, `pyproject.toml`, `uv.lock` | `just py-check` (`vercel`, `pydantic` v2, `httpx`, `ruff`, `pytest`) |
| **C++23 / Bazel 9** | `.bazelversion`, `MODULE.bazel`, `.bazelrc`, `.clang-format` | `just cpp-check` (`abseil-cpp` CRC32C/StatusOr + `googletest`) |
