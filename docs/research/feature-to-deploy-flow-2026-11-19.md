# Feature-to-Deploy Flow: Rust Script + Bun Shell/Spawn/AOT + `xtask` + Fat-LTO (2026-11-19)

## Verified Live Sources (2026-11-19)
- `oven-sh/bun` `main`: `ca7729d01a` (`2026-11-19T08:57:44Z`), local `bun 1.4.5` (`bun build --help`)
- `rust-lang/cargo` `master`: `e353516767` (`2026-11-19T05:47:05Z`), local `cargo 1.102.0-nightly (c07ac2c4d)`
- `rust-lang/rust` `main`: `c884ecfbab` (`2026-11-19T08:00:19Z`), local `rustc 1.102.0-nightly (3ef0c22cd)` (LLVM `23.1.5`)
- `fornwall/rust-script` `main`: 1,662 stars, last pushed `2025-08-16` (15 months inactive vs active `cargo -Zscript`)
- Note: public unauthenticated Context7 endpoint returned monthly quota exceeded on 2026-11-19; all facts verified directly against live GitHub HEAD commits and local tool binaries.

## Tooling Selection Matrix (Zero Overhead, Maximum Parallelism)

| Stage | Primitive | Measured Speed | When to Use |
| :--- | :--- | ---: | :--- |
| **Single `.rs` script** | `rust-script file.rs` | **17 ms** warm / 936 ms cold | Fastest `bun run`-like cached single-file Rust execution |
| **Official Cargo script** | `cargo -Zscript -Zcodegen-backend` (Cranelift) | **47 ms** warm / **510 ms** cold | Standard RFC 3502/3503 `---` frontmatter with `rust-analyzer` + `rustfmt` (`just rs-run <file>`) |
| **Cross-OS shell pipelines** | `Bun.$` (Bun Shell) | ~1–3 ms | Shell globs, pipes, redirects with automatic `${var}` escaping (zero pwsh/bash quoting bugs) |
| **Parallel orchestrators** | `Bun.spawn` (`Promise.all`) / Rust `std::thread::scope` | ~1 ms spawn | Zero shell overhead; direct argv fan-out (`tools/rust_brain.ts`, `xtask/src/main.rs`) |
| **Monorepo CI & Ship** | `cargo xtask ci fast` / `cargo xtask ship` | **6.9 s** CI / parallel ship | Pure-Rust zero-dep orchestrator replacing fragmented TS/shell scripts |
| **Native Rust Release** | `cargo build --release -p fw-bin --bin fw` | **873 KiB** | Fat-LTO (`lto = "fat"`, `codegen-units = 1`, `strip = true`, `panic = "abort"`) |
| **Bun AOT Standalone CLI** | `bun build --compile --aot --trim --no-compile-function-source --no-compile-autoload-dotenv --no-compile-autoload-bunfig` | Single `.exe` | Native machine-code JS/TS AOT with unused APIs trimmed and zero startup `.env`/`bunfig` disk probes |

## End-to-End Feature-to-Deploy Flow (`fw`)

1. **Prototype & Dev (`< 1 s`)**: `just rs-run <file.rs>` or `cargo fw <cmd>` (`[profile.dev] debug = "line-tables-only"`, deps at `opt-level = 2`).
2. **Parallel Verification Gate (`just ci-fast`, ~7 s)**: `purity` + `rust-clippy` (Rust 2024, `clippy::perf = "deny"`) + `rust-test` + `python-ruff` + `bun-test` + `MAP.tsv`.
3. **Parallel Release & Edge Build (`just ship` / `cargo xtask ship`)**:
   - Stage 1: runs `cargo xtask ci fast`.
   - Stage 2 (4 concurrent `std::thread::scope` lanes): `fw-fat-lto` (`fw.exe`), `fw-wasm32` (`fw_wasm.wasm`), `bun-aot-cli` (`dist/fairwind.exe`), and `cf-dry-run` (`wrangler deploy --dry-run`).
4. **Deploy**:
   - **Edge**: `just cf-deploy` (Cloudflare Workers + Static Assets + DO SQLite) & Vercel (`vercel/`).
   - **3-VPS + WSL Fleet**: `flake.nix` (`nix-cachyos-kernel` LTO + `fw-rust-userland-4.2.0` + `scx_lavd`).
   - **Crates.io & Releases**: `just package` + `.github/workflows/release.yml` (Windows MSVC, Linux musl, macOS arm64, Wasm32).
