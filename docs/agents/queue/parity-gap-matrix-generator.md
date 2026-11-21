# Lot: regenerate the parity matrix from code, in Rust

The matrix and its TSV were built with throwaway Bun scripts (not committed: a TypeScript file outside the baseline fails `layout.ts check`). Write the generator as a Rust xtask or `fw` subcommand: npm registry tarballs (component lists), ui.shadcn.com registry index, crates.io and GitHub APIs (versions, paths, line cites), the fw tree at a given commit, and the benchmark TSVs; emit `docs/research/web-vs-rust-parity-2026-11.tsv` and the tables with `--check`.
