<!-- SPDX-License-Identifier: Apache-2.0 -->

# @aphrody/rag-core

<!-- catalog:begin unit -->

> Aphrody RAG core for the web (WebAssembly) and for Bun (bun:ffi): chunking, fusion, eval and vector kernels behind one typed facade.

| Type        | Path                              |
| :---------- | :-------------------------------- |
| Bun package | `packages/ai-os/engines/rag-core` |

**Commands**

```sh
bun run --cwd packages/ai-os/engines/rag-core build:wasm
bun run --cwd packages/ai-os/engines/rag-core typecheck
bun run --cwd packages/ai-os/engines/rag-core test
```

Part of Aphrody: [aphrody-labs/ay](../../../../README.md).
<!-- catalog:end -->

The RAG core of Aphrody (source-aware chunkers, rank fusion, MMR, source weighting, vector kernels,
the retrieval eval harness) behind one typed facade, with two backends over the same Rust source of
truth, `crates/ay-rag-core/src/api.rs`:

| Backend     | Import                   | Use                                                                    |
| :---------- | :----------------------- | :--------------------------------------------------------------------- |
| WebAssembly | `@aphrody/rag-core/wasm` | browser, Bun, Node (the shared `@aphrody/cli-wasm` module)             |
| bun:ffi     | `@aphrody/rag-core/ffi`  | Bun backend (`crates/ay-rag-core`, exported by `aphrody-ffi` in the OS repository) |

The native desktop app links `ay-rag-core` directly as a Rust crate (no binding layer).

```ts
import { loadRagWasm } from "@aphrody/rag-core/wasm"; // web
const rag = await loadRagWasm();
rag.chunk("docs/guide.md", markdown, { profile: "shenron_parity" });
rag.topKCosineF32(query, matrix, 384, 10);

import { openRagFfi } from "@aphrody/rag-core/ffi"; // Bun backend
const native = openRagFfi(); // APHRODY_RAG_LIBRARY or the Aphrody lib store
```

Both return the same `RagCore` object. Adding a capability is one row in `api.rs`; the manifest
(`rag.manifest`) lists every operation and `test/` fails when this facade lacks one.

Build the browser module with `bun run build:wasm`; this delegates to `@aphrody/cli-wasm`,
whose single binary is `aphrody_command_wasm_bg.wasm`. CLI and typed RAG calls share its
loader and memory. The `loadRagWasm` and `loadRagWasmSync` APIs remain unchanged; explicit
bytes or module inputs must now be the CLI module. The old generated `wasm/*` resource
exports are replaced by `@aphrody/cli-wasm/wasm/*`. No second module is copied into this package.
