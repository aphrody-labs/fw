# ay

The SDK of Aphrody and the registry of its workspace: the C ABI and the freestanding corelib, the design tokens, the cloud and edge tooling (Cloudflare, Vercel, Bun), the reverse-engineering suite, the Auto product and the workspace orchestration over the sibling repositories (`fw`, `aphrody`, `nie`, `shenron`, ...). Names and domains come from `ay.toml`; the agent contract, including the dependency direction between repositories, is `AGENTS.md`.

## Layout

| Path | Holds | Gate |
|---|---|---|
| `abi/`, `corelib/` | C ABI, freestanding corelib (C, assembly) | `bun tools/abi.ts --check`, `bun tools/corelib.ts` |
| `crates/` | Rust workspace (`ay-core`, `ay-re`, `ay-cf`, `ay-media`, `ay-design`, `ay-vault`, `auto-*`, ...) | `cargo check --workspace` (on the build host) |
| `packages/` | Bun packages (cf-control-plane, edge-stack, env-kit, org-mirror, auto-*, ...) | `bun test ./packages` |
| `apps/` | Programs: `ay` CLI, `edge` Worker, `cf-gateway`, `tools-release`, `auto-*` | `bun test ./apps/ay` |
| `python/` | Python workspace (uv): Cloudflare SDK and control-plane models | `just py-check` |
| `cpp/` | C++23 units built with Bazel | `just cpp-check` |
| `kotlin/` | Gradle workspace: the Aphrody Compose theme | `bun tools/compose-check.ts` |
| `config/` | Shell and CLI configs installed by `tools/shells.ts` | `bun tools/shells.ts --check` |
| `locales/` | Message catalogs of the Auto product | `bun test ./packages/auto-i18n` |
| `tools/` | Typed Bun scripts: checks, generators, Cloudflare operations, workspace orchestration | `bun test ./tools/` |
| `brand/` | Design tokens (Material 3) and generated themes | `bun tools/tokens-compose.ts --check` |
| `docs/` | Spec, architecture, ADRs, feature matrix | `bun tools/features.ts --check` |
| `third_party/` | Upstream forks as submodules (`submodules.toml`, relative URLs) | `bun tools/submodules.ts --check` |

Per-directory tier and gate: `map.toml`. Generated output goes to the git-ignored `.private/`.

## Workspace

`ay.toml` [repos] is the registry of the live repositories (one slug each, all under `aphrody-labs`; `[retired]` lists the archived ones). They are sibling
checkouts under one root, in stack order `fw`, `aphrody`, `ay`, `nie`, `shenron`. `tools/workspace.ts` and `workspace.just` (imported by the root
`justfile`) report their status, audit them against the registry, pull them and run one repository's recipes. Each upstream fork is checked out by exactly one repository.

## Commands

```sh
bun install        # workspace dependencies
bun run check      # every gate: matrix, paths, private-gate, ABI, tokens, edge config, typecheck, tests
bun run features   # regenerate docs/features.md from docs/features.toml
bun run abi        # regenerate abi/ and docs/abi.md
bun run map        # regenerate MAP.tsv
```

Rust builds and tests run on the build and test hosts, never on the PC (`ay.local.toml` `[hosts]`).

## Read first

- [`docs/spec.md`](docs/spec.md): products and definition of done.
- [`docs/architecture.md`](docs/architecture.md), [`docs/adr/`](docs/adr/README.md): tree, toolchain, decisions.
- [`docs/features.toml`](docs/features.toml): every feature with its owner and proof (`docs/features.md` is generated).
- [`docs/edge.md`](docs/edge.md), [`docs/release.md`](docs/release.md): the edge Worker and the release workflow.
- [`docs/standard.md`](docs/standard.md): organization conventions and CI policy.
