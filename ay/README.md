# Aphrody

The Aphrody SDK and platform workspace: C ABI and freestanding corelib, Rust crates, Bun packages, AI OS runtime, shell and PTY, reverse-engineering core, Linux platform pins, and design tokens.

| Product | Replica of | Status |
|---|---|---|
| Aphrody OS | The Googlebook experience | see [features](docs/features.md) |
| Aphrody Browser | Chromium-based browsers | see [features](docs/features.md) |
| Aphrody Search | google.com | see [features](docs/features.md) |
| Aphrody Cloud | Cloudflare Workers + Vercel | see [features](docs/features.md) |
| Craft Anything | storytold/craft-anything | see [features](docs/features.md) |

## Components

- **C ABI & corelib**: `abi/`, `corelib/`, `crates/ay-core/capi` ([ABI reference](docs/abi.md))
- **AI OS & agent harness**: `packages/ai-os`, `crates/ay-a2a*`, `crates/ay-agent-*`, `crates/ay-rag*`, `crates/ay-llama*`, `crates/ay-store`, `crates/ay-vault`, `apps/agents`, `apps/ai-gateway`
- **Shell, PTY & environment**: `crates/ay-sh`, `packages/omniu`, `config/`, `packages/env-kit`
- **Reverse engineering & Linux platform**: `crates/ay-re`, `crates/ay-ghidra-bun`, `packages/ghidra`, `packages/py2bun`, `os/linux`, `tools/linux-*.ts`
- **Chromium & web validation**: `crates/ay-chromium`, `packages/chromium`, `packages/chromium-releases`, `apps/chromium-dash`, `packages/wck` ([Chromium guide](docs/chromium.md), [Chromium Dash](docs/chromium-dash.md))

## Documentation

- [Specification](docs/spec.md), [architecture](docs/architecture.md), [ABI](docs/abi.md), [decisions](docs/adr/README.md)
- [Feature matrix](docs/features.md): product scope, references, and verified proofs
- [Organization standard](docs/standard.md): repository conventions and CI policy

## Commands

```sh
bun install        # install workspace dependencies
bun run check      # feature matrix, paths, private-gate, ABI, typecheck, and Bun tests
bun test           # run Bun test suites
bun fmt            # format workspace files
bun run features   # regenerate docs/features.md
bun run abi        # regenerate abi/ and docs/abi.md
```
