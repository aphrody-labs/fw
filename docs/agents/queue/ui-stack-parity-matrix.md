# Lot: parity matrix, web stacks against pure Rust

Order of the owner, 2026-11-21: an audited parity and equivalence matrix between the web stacks we use or reference and their pure-Rust equivalents.

## Stacks to compare

| Web stack | Pure-Rust candidates to compare |
|---|---|
| Base UI (@base-ui/react) | GPUI components (gpui-component, our aphrody UI crates) |
| shadcn/ui | GPUI components, same candidates as Base UI |
| better-auth | the axum/actix auth ecosystem, our fw/ay auth |
| Elysia (Bun) | axum, our fw-proto server |
| Next.js | Leptos and Dioxus fullstack, our NSH |
| three.js | Bevy, wgpu, our gpu-bench numbers |
| motion.dev | GPUI animations and springs, Material 3 motion in fw-m3 |
| Material UI (MUI) | our Material 3 implementation in fw-m3 and fw-material |

## Matrix

- **Rows:** one per feature, taken from each web stack's own docs and component list at today's version.
- **Columns:** the web stack, the best Rust equivalent, and our implementation.
- **Cells:** present, partial or missing, with a source link (`repo@commit path:line`).
- **Benchmarks:** measured where possible: bundle and binary size, startup, frame time, request latency.

## Output

- **Matrix:** `docs/research/web-vs-rust-parity-2026-11.md`.
- **Gaps:** in `docs/agents/queue/`, ordered by user impact.
- **Decision per row:** adopt upstream, port, or keep the web stack. Each decision gives its reason.

## Done (2026-11-21)

- Matrix: `docs/research/web-vs-rust-parity-2026-11.md`, full cells in `docs/research/web-vs-rust-parity-2026-11.tsv` (242 rows over Base UI, shadcn/ui, Material UI, better-auth, Elysia, Next.js, three.js, motion.dev), each row with a cited web cell, ours, best Rust equivalent and a decision with its reason.
- Benchmarks measured on prd and fr1: request cost (axum, actix-web, Elysia, `fw serve`), SSR of a 100-row table (Leptos, Dioxus, Next.js), artefact sizes (Rust binaries, JS bundles, Leptos and Dioxus wasm), scrypt per hash against better-auth. Raw runs: `docs/research/web-vs-rust-parity-2026-11-bench.tsv`.
- Gap lots, ordered by user impact: `parity-gap-auth-compat-and-flows.md` (includes a defect: NFKC no-op in the scrypt compat hash), `parity-gap-fw-proto-on-axum.md`, `parity-gap-gpui-kit-sync-and-components.md`, `parity-gap-motion-layout-gesture-scroll.md`, `parity-gap-blitz-native-html.md`, `parity-gap-web-fullstack-rust.md`, `parity-gap-three-bevy-frame-time.md`, `parity-gap-matrix-generator.md`.

## Remaining

- Frame time three.js against Bevy (gpu-bench Suite B, no results yet).
- Wasm sizes of wgpu and Bevy and the native GPUI hello binary: builds on fr1 were stopped at load average 70 to 100 (Leptos and Dioxus wasm finished); rerun when the node is idle.
- Latency percentiles at low host load (the hosts ran at load 27 to 100).
