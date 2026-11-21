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
