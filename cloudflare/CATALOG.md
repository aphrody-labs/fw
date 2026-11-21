# Cloudflare ecosystem catalog (Nov 2026)

Verified inventory of Cloudflare Edge OS, Computer, Agent, Sandbox, Rust/Wasm, Browser, and Data Platform building blocks consolidated into `fw-edge` (`cloudflare/worker.ts`, `cloudflare/wrangler.jsonc`, `cloudflare/wrangler.fullstack.jsonc`).

The live machine-readable version of this catalog is served at `GET /os/catalog` (with optional `?layer=` filtering) and exposed through the MCP tool `os_catalog`.

## Architecture Overview

`fw-edge` combines a zero-dependency Web Standards protocol gateway with a per-tenant SQLite Durable Object kernel (`FwOSAgent`) and a 3-tier execution router:

1. **Tier-0 (`do-posix-shell`)**: In-DO virtual POSIX shell backed by SQLite `vfs_nodes` (and optional R2 blob spillover via `env.VFS_BLOBS`). Zero container cold start.
2. **Tier-1 (`dynamic-worker-loader`)**: Ephemeral V8 isolate loaded via `env.LOADER.load()` (`worker_loaders`) with `globalOutbound: null` by default for CodeMode TypeScript/JavaScript orchestration.
3. **Tier-2 (`cloudflare-sandbox-microvm`)**: Dedicated Linux microVM via `env.SANDBOX` (`@cloudflare/sandbox@1.0.3` on `@cloudflare/containers@0.3.7`) using the explicit `argv` process handle contract (`exec(argv)` -> `output()`).

Every VFS write, execution dispatch, and learning append passes through the fail-closed **Gatekeeper** policy engine (`gatekeeper_rules` in DO SQLite).

## Verified Nov 2026 Ecosystem Matrix

| Component | Version | Layer | Reference URLs | Role & Fw Wiring |
| --- | --- | --- | --- | --- |
| `cloudflare/cloudflare-os` | 2026.11 | `os-control-plane` | https://github.com/cloudflare/cloudflare-os | Multi-worker AI OS workspace, gadget sandboxing, Access SSO, and Gatekeeper capability policies. Wired into `FwOSAgent` Gatekeeper (`/os/gatekeeper`, `gatekeeper_rules`) with fail-closed deny precedence. |
| `cloudflare/computer` + `@cloudflare/dofs` | 2026.11 | `vfs-kernel` | https://github.com/cloudflare/computer · https://www.npmjs.com/package/@cloudflare/dofs | Authoritative per-tenant SQLite VFS inside a Durable Object with pluggable execution runtimes. Wired into `FwOSAgent` (`vfs_nodes` table, `/os/vfs/{ls,read,write,stat,exec}`, transparent `env.VFS_BLOBS` R2 spillover). |
| `cloudflare/agents` | `0.33.0` | `agent-runtime` | https://github.com/cloudflare/agents · https://developers.cloudflare.com/agents/ | Durable Object Agent SDK (`Agent`, `@callable` RPC, `AIChatAgent`, `runFiber`/`stash`, scheduling, `createMcpHandler`, WebSocket hibernation). Mirrors the per-tenant DO coordination model and `blockConcurrencyWhile` schema bootstrap. |
| `@cloudflare/codemode` | `0.5.6` | `execution-tier` | https://github.com/cloudflare/agents/tree/main/packages/codemode · https://www.npmjs.com/package/@cloudflare/codemode | Code-mode tool orchestration where models execute code inside `DynamicWorkerExecutor` instead of sequential tool round-trips. Wired into Tier-1 `/os/vfs/exec` via `env.LOADER.load({ globalOutbound: null })`. |
| `@cloudflare/sandbox` | `1.0.3` (`@next`) | `execution-tier` | https://github.com/cloudflare/sandbox-sdk · https://developers.cloudflare.com/sandbox/ | Isolated Linux microVMs on Cloudflare Containers with explicit `argv` process handles (`exec`, `output`, `waitForExit`, `kill`), terminals, and mounts. Wired into Tier-2 `/os/vfs/exec` via `env.SANDBOX` (`wrangler.fullstack.jsonc`). |
| `@cloudflare/containers` | `0.3.7` | `execution-tier` | https://github.com/cloudflare/containers · https://developers.cloudflare.com/containers/ | Durable Object-backed container lifecycle, routing, scaling, and Preview isolation. Configured under `containers[]` in `cloudflare/wrangler.fullstack.jsonc`. |
| `cloudflare/workers-rs` | `worker@0.9.0` | `vfs-kernel` | https://github.com/cloudflare/workers-rs · https://crates.io/crates/worker | Rust-to-WebAssembly Workers SDK for native/low-level edge logic. Serves as the Wasm companion target alongside Fw's Fat-LTO Rust crates (`native/fw-abi`, `native/fw-pe`). |
| Browser Run + Kitesurf | 2026.11 | `browser` | https://developers.cloudflare.com/browser-rendering/ · https://github.com/cloudflare/kitesurf | Managed headless Chromium plus Kitesurf (`cloudflare/kitesurf`), an Apache-2.0 Rust/Wasm stateless CDP agent browser (`Blitz` + `Stylo`) running in V8 isolates. Wired via optional `env.BROWSER` (`browser` binding). |
| `cloudflare/vibesdk` | 2026.11 | `os-control-plane` | https://github.com/cloudflare/vibesdk | Open-source vibe-coding platform combining SpaceDO workspaces, Dynamic Workers, and isolated preview deployments. Informs Fw's per-tenant workspace + Tier-1 isolate dispatch. |
| `cloudflare/moltworker` | 2026.11 | `agent-runtime` | https://github.com/cloudflare/moltworker | Self-hosted personal AI agent middleware running on Workers, Sandbox microVMs, and R2 storage. Matches Fw's personal tenant DO + R2 blob spillover + Sandbox escalation model. |
| `hono` | `4.13` | `web-protocol` | https://github.com/honojs/hono · https://hono.dev/ | Web Standards Fetch router and middleware for Workers APIs. `fw-edge` exposes a clean Web Standards `fetch(request, env)` entrypoint directly mountable behind Hono or standalone. |
| `just-bash` | `3.6.1` | `execution-tier` | https://github.com/vercel-labs/just-bash · https://www.npmjs.com/package/just-bash | In-isolate virtual POSIX bash interpreter operating directly over a virtual filesystem. Modeled by Fw Tier-0 (`pwd`, `ls`, `cat`, `echo`, `stat`, `mkdir`, `touch`, `rm`, `cp`, `mv`, `wc`, `head`, `tail`, `grep`, `find`, `cd`, `env`, `uname`, `whoami`, redirection, and `&&`/`;` chaining). |
| `@modelcontextprotocol/server` | `2.x` | `web-protocol` | https://github.com/modelcontextprotocol/typescript-sdk · https://modelcontextprotocol.io/ | Official MCP v2 TypeScript server specification over Streamable HTTP JSON-RPC 2.0. Implemented directly at `POST /mcp` (`initialize`, `tools/list`, `tools/call`, `ping`). |
| Cloudflare Basin | GA 2026 | `data-stream` | https://developers.cloudflare.com/basin/ · https://developers.cloudflare.com/basin-pipelines/ | Basin Pipelines (1 GB/s ingest via `pipelines[].stream`), Basin Catalog (R2 Apache Iceberg tables), and Basin SQL. Wired as `env.TELEMETRY_PIPELINE.send()` on `/api/learn/events`. |
| Cloudflare K2 Streams | Beta 2026 | `data-stream` | https://developers.cloudflare.com/k2/ · https://developers.cloudflare.com/k2/features/produce/ | Durable at-least-once event log (`k2[].stream`) with decoupled producers and replayable subscriptions. Wired as `env.EVENT_LOG` on `/api/learn/events`. |

## Protocol Gateway Endpoints

| Endpoint | Methods | Protocol / Function |
| --- | --- | --- |
| `/health` | `GET` | Worker version, compatibility date, environment, and live binding inventory |
| `/openapi.json` | `GET` | OpenAPI 3.1.0 schema covering all gateway and Edge OS routes |
| `/.well-known/agent.json` | `GET` | A2A AgentCard (`protocolVersion: "1.0.1"`) with skills and tier capabilities |
| `/v1/models` | `GET` | OpenAI-compatible Workers AI + Fw router model catalog |
| `/v1/chat/completions` | `POST` | OpenAI-compatible chat backed by `env.AI` with deterministic edge fallback |
| `/v1/embeddings` | `POST` | OpenAI-compatible embeddings (`@cf/baai/bge-base-en-v1.5` or deterministic 64-dim fallback) with optional `env.MEMORY_VEC` Vectorize upsert |
| `/mcp` | `POST` | Streamable HTTP MCP JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`, `ping`) |
| `/acp` | `POST` | ACP JSON-RPC 2.0 (`initialize`, `session/new`, `session/prompt`, `ping`) |
| `/a2a` | `POST` | A2A JSON-RPC 2.0 (`tasks/send`, `tasks/get`, `ping`) |
| `/os/catalog` | `GET` | Structured JSON catalog (filterable via `?layer=`) |
| `/os/tiers` | `GET` | 3-tier execution matrix and binding readiness |
| `/os/vfs/{ls,read,write,stat,exec}` | `GET`, `POST` | Per-tenant SQLite VFS + 3-tier execution routed to `env.FW_OS` DO |
| `/os/gatekeeper` | `GET`, `POST` | List Gatekeeper rules, check capabilities, or upsert tenant policy rules |
| `/api/learn/events` | `GET`, `POST` | Persistent autonomous learning log (DO SQLite + K2 + Basin Pipelines + D1) |
| `/api/learn/status` | `GET` | Learning log counts, breakdown by kind, audit count, and active sink status |

## Binding Summary (`FwWorkerEnv`)

- `AI`: Workers AI inference & embeddings (`@cf/*`)
- `FW_OS`: SQLite Durable Object namespace (`FwOSAgent`)
- `LOADER`: Dynamic Worker Loader (`worker_loaders`) for Tier-1 CodeMode isolates
- `SANDBOX`: Sandbox 1.0 Durable Object (`containers[]`) for Tier-2 Linux microVMs
- `VFS_BLOBS`: R2 bucket for payloads exceeding 64 KiB or explicit spillover
- `META_DB`: D1 relational database for cross-tenant metadata mirroring
- `MEMORY_VEC`: Vectorize index for semantic memory upserts from `/v1/embeddings`
- `BROWSER`: Browser Run binding (Chromium / Kitesurf)
- `EVENT_LOG`: K2 durable event stream (`k2[].stream`)
- `TELEMETRY_PIPELINE`: Basin Pipelines ingestion stream (`pipelines[].stream`)
- `FW_ENV`: Deployment environment string (`production`, `staging`, `preview`)
