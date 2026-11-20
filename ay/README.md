# ay

The Aphrody SDK, built for agents: C ABI and corelib, Rust crates, Bun packages, the AI OS runtime, the unified edge Worker, and the design tokens. Names, hosts and domains come from `ay.toml`; the agent contract is `AGENTS.md`.

## Layout

| Path | Holds | Gate |
|---|---|---|
| `abi/`, `corelib/` | C ABI, freestanding corelib (C, assembly) | `bun tools/abi.ts --check`, `bun tools/corelib.ts` |
| `crates/` | Rust workspace (`ay-*`) | `cargo check --workspace` (on the build host) |
| `packages/` | Bun packages (AI OS, chromium, cf-control-plane, ...) | `bun test ./packages` |
| `apps/` | Programs: `ay` CLI, `edge` Worker, `cf-gateway`, `agents`, `ai-gateway`, `auto-*` | `bun test ./apps/ay` |
| `tools/` | Typed Bun scripts: checks, generators, Cloudflare operations | `bun test ./tools/` |
| `brand/` | Design tokens (Material 3) and generated themes | `bun tools/tokens-compose.ts --check` |
| `docs/` | Spec, architecture, ADRs, feature matrix | `bun tools/features.ts --check` |
| `os/`, `third_party/` | Linux layer, upstream submodules | `bun tools/submodules.ts --check` |

Per-directory tier and gate: `map.toml`. Generated output goes to the git-ignored `.private/`.

## Commands

```sh
bun install        # workspace dependencies
bun run check      # every gate: matrix, paths, private-gate, ABI, tokens, edge config, typecheck, tests
bun run features   # regenerate docs/features.md from docs/features.toml
bun run abi        # regenerate abi/ and docs/abi.md
bun run map        # regenerate MAP.tsv
```

Rust builds and tests run on the build and test hosts, never on the PC (`ay.toml` `[hosts]`).

## Read first

- [`docs/spec.md`](docs/spec.md): products and definition of done.
- [`docs/architecture.md`](docs/architecture.md), [`docs/adr/`](docs/adr/README.md): tree, toolchain, decisions.
- [`docs/features.toml`](docs/features.toml): every feature with its owner and proof (`docs/features.md` is generated).
- [`docs/edge.md`](docs/edge.md), [`docs/release.md`](docs/release.md): the edge Worker and the release workflow.
- [`docs/standard.md`](docs/standard.md): organization conventions and CI policy.
