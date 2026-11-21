# API, schema and binding contracts for every product (research, 2026-11-21)

The owner asked for complete GraphQL, Open Graph, REST and OpenAPI coverage, and the rest, in every product. Each product gets schemas, types and contracts that work across languages, OSes, frameworks, FFI and the C ABI, with AST and IR included.

Every version below was read live on 2026-11-21:

- spec repositories through `gh api repos/<o>/<r>/releases/latest`;
- crates through `crates.io/api/v1/crates/<name>`, where `dl90` is the download count over the last 90 days;
- Open Graph through https://ogp.me.

## What exists today (origin/main of each repo)

| Repo | Contracts present |
|---|---|
| fw | `crates/fw-a2a/proto/a2a.proto` and the google.api protos; `crates/fw-edge/data/openapi_{paths,description}.json`; the MCP engine `crates/fw-proto/src/mcp.rs` (152/152 on the suite at 2026-07-28); 17 C headers; `schemars` in fw-proto, fw-toolcall, fw-m3 and fw-a2a |
| aphrody | Zed's collaboration RPC in `crates/proto/proto/*.proto` (11 files); 2 C headers; JSON Schema through `schemars` in settings_content, agent, gpui and others |
| ay | `docs/auto/openapi.json`; the WebAssembly component interface `corelib/wit/ay-corelib.wit` with WASI `io` and `cli` deps; 8 C headers |

Missing everywhere:

- **No single source.** Each product has no one source of truth for its contracts; the OpenAPI files are written by hand or generated in one place.
- **Missing formats:**
  - no GraphQL schema;
  - no AsyncAPI for events and queues;
  - no Open Graph metadata contract for the sites;
  - no generated TypeScript, Kotlin, Python or C# types.
- **Unchecked bindings:** no ABI check on the C headers, and no AST or IR contract.

## Current standards

| Layer | Standard | Live version |
|---|---|---|
| HTTP APIs | OpenAPI (OAI/OpenAPI-Specification) | 3.2.1, 2026-09-10 |
| Query APIs | GraphQL (graphql/graphql-spec) | September2026, 2026-11-12 |
| Query APIs | GraphQL over HTTP (graphql/graphql-over-http) | September2026, 2026-11-13 |
| Events and queues | AsyncAPI | v3.1.0, 2026-01-31 |
| Data shapes | JSON Schema 2020-12 (json-schema-org/json-schema-spec, no GitHub release; head 2026-11-19) | 2020-12 |
| RPC | Protocol Buffers | v36.2, 2026-09-17 |
| RPC | Connect: `connectrpc` crate | 0.9.1 |
| RPC | gRPC: `tonic` crate | 0.14.9 |
| Agent tools | MCP | 2026-07-28 |
| Agent to agent | A2A | v1.0.1, 2026-05-28 |
| Service models (AWS-style IDL) | Smithy | 1.75.0, 2026-11-12 |
| Link previews | Open Graph protocol, ogp.me | `og:title`, `og:type`, `og:image` (with `:url`, `:secure_url`, `:type`, `:width`, `:height`, `:alt`), `og:url`, `og:description`, `og:audio`, `og:determiner` |
| Wasm components | Component Model WIT (WebAssembly/component-model, no release; head 2026-11-19) | `wit-bindgen` 0.63.0 |

## Rust tools

### Generate contracts from code

Rust types are the source, in line with the derive-from-code rule.

| Crate | Version | dl90 | Role |
|---|---|---|---|
| `schemars` | 1.2.3 | 265M | JSON Schema 2020-12 from Rust types |
| `utoipa` | 6.0.0 | 24M | OpenAPI from axum and other handlers |
| `aide` | 0.15.1 | 1.5M | OpenAPI, alternative to utoipa; utoipa leads on use |
| `async-graphql` | 7.2.1 | 4.5M | GraphQL schema and server; emits SDL |
| `prost` | 0.14.6 | | protobuf codegen |
| `tonic` | 0.14.9 | | gRPC |
| `buffa` | 0.9.2 | | protobuf codegen |
| `connectrpc` | 0.9.1 | | Connect RPC |

### Generate code from contracts

| Crate | Version | Role |
|---|---|---|
| `typify` | 0.10.1 | Rust types from JSON Schema |
| `progenitor` | 0.17.0 | Rust clients from OpenAPI |
| `cynic` | 3.14.0 | typed GraphQL clients |

### Cross-language types and FFI

| Crate | Version | dl90 | Target |
|---|---|---|---|
| `ts-rs` | 12.0.1 | 11.5M | TypeScript |
| `specta` | 1.0.5 | 3.4M | TypeScript |
| `typeshare` | 1.0.5 | 1.9M | TS, Kotlin, Swift, Go, C# |
| `uniffi` | 0.32.2 | 7.1M | Kotlin, Swift, Python bindings |
| `diplomat` | 0.17.0 | 12.2M | C, C++, JS, Kotlin, Dart from one bridge |
| `cbindgen` | 0.29.4 | 21M | C headers |
| `safer-ffi` | 0.1.13 | | C ABI |
| `interoptopus` | 0.16.6 | | C#, C, Python |
| `wit-bindgen` | 0.63.0 | 152M | Wasm components |

### AST and IR

| Crate | Version | Role |
|---|---|---|
| `tree-sitter` | 0.27.2 | source AST in every language; the kernel lot reuses it |
| `naga` | 30.0.2 | shader IR, already used by the wgpu stack |
| `inkwell` | 0.11.0 | LLVM IR |
| `wasmparser` / `walrus` | | Wasm IR |
| rustc `stable_mir` (rustc_public) | | Rust MIR, through the pinned toolchain 1.100.0 |

## Target design (for the implementation lot)

1. **One contract source per product.**
   - **Generated from Rust:** HTTP (OpenAPI 3.2.1), GraphQL SDL, JSON Schema, AsyncAPI for queues and Workers events, WIT for Wasm components, and C headers. The sources are annotated Rust types (`schemars`, `utoipa`, `async-graphql`) and the `extern "C"` items (`cbindgen` or `diplomat`).
   - **Hand-written:** only for external protocols we implement, such as A2A protos and MCP; these are vendored at the upstream release.
2. **`fw contracts gen|check`** writes every artifact under `contracts/<product>/`, with fixed layout and file names.
   - **Generated clients and types per language:** TS for Bun, through ts-rs or specta; Kotlin and Python through uniffi; C# through interoptopus; C and C++ through diplomat or cbindgen.
   - **CI check:** `check` fails on drift and on breaking changes. It compares against the last release tag: oasdiff-style for OpenAPI, GraphQL schema diff, `buf breaking` for protos, and an ABI check (`abi-dumper` or `cargo-semver-checks`) for the C ABI.
3. **Coverage:** REST and GraphQL on every server surface. That covers fw-proto, fw-edge and ay-edge, plus the aphrody collab and services and shenron's API. A2A and MCP are generated from the same tool and agent registry.
4. **Open Graph and structured data.** Every page of the sites and web apps carries complete `og:*` tags and JSON-LD, generated from `product.toml`. A test reads the rendered head of each page.
5. **AST and IR contracts.** These are file stores fw can query (everything is a file):
   - tree-sitter graphs of every product, shared with the kernel lot's graph store;
   - Naga IR for every WGSL shader;
   - Wasm IR summaries;
   - the MIR call graph of the Rust workspaces.
6. **OS and framework coverage.** Contracts are OS-neutral. FFI artifacts are built and tested on the current targets: Windows 11 (msvc), recent Linux (gnu and musl) and wasm32 (browser and WASI). macOS is skipped for now.
