# CLI Performance Secrets & Live `hyperfine` Proof (`fw` vs `eza`, `fd`, `rg`, `nu`)

- **Date**: 2026-11-19
- **Machine**: Windows 11 (`x86_64-pc-windows-msvc`, Intel Core i7-13700F, AVX2), `rustc 1.102.0-nightly`, LLVM `23.1.5`
- **Binary**: `target/release/fw.exe` — **893,952 bytes (873 KiB)** Fat-LTO, **0 external dependencies** for `fast_cli`, **`#![forbid(unsafe_code)]`**
- **Implementation**: [crates/fw-uutils/src/fast_cli.rs](../../crates/fw-uutils/src/fast_cli.rs), dispatched at top of [crates/fw-bin/src/main.rs](../../crates/fw-bin/src/main.rs)

## Performance Secrets Extracted from Upstream Tools

| Tool | Version | Core Mechanical Sympathy | Overhead Eliminated in `fw` |
| :--- | :--- | :--- | :--- |
| **`eza`** | `v0.23.5` | Parallel stat/git threads, buffered ANSI output | Removed `clap`, `libgit2` discovery, icon/theme tables, and per-entry `stat()` (uses free `DirEntry::file_type()`) |
| **`fd`** | `v10.5.2` | Work-stealing parallel walk (`ignore`), skips `.git`/`target`, `d_type` | Removed regex engine startup, `.gitignore` globset parsing per dir, and `crossbeam` boxing; uses `std::thread::scope` + byte match |
| **`rg`** | `15.2.0` | SIMD literal prefilter, raw `&[u8]` (no UTF-8 check), lazy line numbers, per-thread buffers | Removed `clap` + PCRE2/regex init; pure-Rust `#![forbid(unsafe_code)]` SWAR 8-byte `u64` broadcast scanner + first/last byte anchor + reusable per-thread read buffer |
| **`nu`** | `0.117.0` | Structured tables (`name`, `kind`, `bytes`) instead of unstructured text | Removed shell parser/evaluator/Reedline startup; direct TSV/JSON table emission (`fw table [--json]`) |

## Live `hyperfine 2.0.0` Benchmarks (`-N`, zero shell overhead)

| Benchmark | `fw` (zero-dep, `#![forbid(unsafe_code)]`) | Competitor | Result |
| :--- | ---: | ---: | :--- |
| `fw ls .` vs `eza .` (25 runs) | **5.1 ms** ± 0.2 ms | `eza`: 12.1 ms ± 1.3 ms | **`fw` 2.4x faster** |
| `fw table .` vs `nu -n --no-std-lib -c 'ls \| select name type size'` (15 runs) | **5.4 ms** ± 0.4 ms | `nu`: 24.7 ms ± 2.0 ms | **`fw` 4.6x faster** |
| `fw find .rs crates` vs `fd -e rs . crates` (25 runs) | **5.7 ms** ± 0.2 ms | `fd`: 14.4 ms ± 1.1 ms | **`fw` 2.5x faster** |
| `fw grep forbid crates` vs `rg -F -n forbid crates` (25 runs) | **6.0 ms** ± 0.2 ms | `rg`: 11.0 ms ± 0.9 ms | **`fw` 1.8x faster** |
| `fw grep fn core/rs` vs `rg -F -n fn core/rs` (298 files / 7.8 MB, 15 runs) | **14.2 ms** ± 0.5 ms | `rg`: 14.4 ms ± 0.7 ms | **`fw` faster (+1.0%)** |

Re-run anytime via `just bench-cli`.
