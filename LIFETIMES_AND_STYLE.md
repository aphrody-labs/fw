# Fairwind Low-Level Rust, Lifetimes & Zero-Abstraction Contract

Modeled on Jarred Sumner's *Rewriting Bun in Rust* (`https://bun.com/blog/bun-in-rust`, 2026-07-08).

## 1. Zero-Abstraction Low-Level Rust Rules

1. **Flat data, explicit memory**: Prefer flat `#[repr(C)]` / plain structs, `&[u8]` slices, fixed arrays, and stack buffers over `Box<dyn Trait>`, `Arc<Mutex<...>>` cascades, or proc-macro DSLs.
2. **Memory safety without GC tax**: `#![forbid(unsafe_code)]` on all workspace crates (`crates/fw-*`) except FFI/ABI (`native/fairwind-abi`) and minimal kernel TCB (`os/fairwind-os` / `ostd`).
3. **Zero hidden panics or silent drops**: Never `.unwrap()` on fallible I/O or user input in production paths; never put side effects inside `debug_assert!`; always check slice lengths and alignment explicitly.
4. **Zero-warning scientific compilation**: Every crate must compile and pass independently in `< 2 s`:
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo build --release -p fw-bin` (Fat-LTO, `codegen-units = 1`, `strip = true`, `panic = "abort"`)
   - `cargo build --release --target wasm32-unknown-unknown -p fw-wasm`
   - `cargo check --target x86_64-unknown-linux-musl -p fw-bin`
5. **No stubs, no excuses**:
   > *"If you need a paragraph-long comment to justify why the workaround is OK, the code is wrong - fix the code."*
6. **0 tests skipped or deleted**: Never delete, comment out, or `#[ignore]` a failing test to turn CI green.

## 2. Token-Minimal Agent Navigation

1. Read `MAP.tsv` first (< 600 tokens) instead of scanning 9,000+ files.
2. Default `rg` and `fd` automatically skip vendored sub-repos via `.rgignore` / `.fdignore` (pass `--no-ignore` only when auditing vendored trees).
3. Commit only explicit file paths (`git add <paths>`); never run `git stash` or `git reset --hard` in multi-agent sessions.
