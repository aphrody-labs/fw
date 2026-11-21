# Rust style and lifetimes

Modeled on Jarred Sumner's *Rewriting Bun in Rust* (`https://bun.com/blog/bun-in-rust`).

## Rules

1. **Flat data, explicit memory**: prefer plain or `#[repr(C)]` structs, `&[u8]` slices, fixed arrays and stack buffers over `Box<dyn Trait>`, `Arc<Mutex<...>>` cascades and proc-macro DSLs.
2. **Unsafe**: new crates start with `#![forbid(unsafe_code)]`. A crate that needs `unsafe` (FFI, `native/fw-abi`, `os/fw-os`) scopes it to the smallest module and documents it with `// SAFETY:`. `git grep -L "forbid(unsafe_code)" -- "crates/*/src/lib.rs"` lists the crates that do not forbid it.
3. **No hidden panics or silent drops**: no `.unwrap()` on fallible I/O or user input in production paths; no side effects inside `debug_assert!`; check slice lengths and alignment explicitly.
4. **Release profile** (`Cargo.toml`): `lto = "fat"`, `codegen-units = 1`, `strip = true`, `panic = "abort"`.
5. **Gates**: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release --target wasm32-unknown-unknown -p fw-wasm`, and `cargo check --target x86_64-unknown-linux-musl -p fw-bin` (the last two run in `.github/workflows/ci.yml`, the wasm32 build also in `cargo xtask ci all`).
6. **No stubs**: if a workaround needs a paragraph of comment to justify it, fix the code instead.
7. **Tests**: never delete, comment out or `#[ignore]` a failing test to turn CI green.

## Agent navigation

1. Read `MAP.tsv` before scanning the tree.
2. `rg` and `fd` skip vendored sub-repos through `.rgignore` and `.fdignore`; pass `--no-ignore` only when auditing vendored trees.
3. Commit explicit paths (`git add <paths>`); never `git stash` or `git reset --hard` in multi-agent sessions.
