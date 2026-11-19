# ADR-14: One edge Worker for the ecosystem

Status: accepted 2026-11-19. Extends 0012; supersedes nothing. Applies the owner's directive of 2026-11-19 (consolidate assets, one Worker,
automatic deploy, release and download for every target, WebAssembly on Cloudflare).

## Context
Read live on 2026-11-19: the account runs 18 Worker scripts; six of them belong to this decision (`aphrody-worker` with `aphrody-learn`, `auto`,
`shenron-cdn`, `ay-cf-gateway`, `ay-tools-release`; `fw-edge` is declared in the fw repository and not deployed). Their routes, buckets and
bindings overlapped (`ASSETS` meant three buckets), `ay-tools-release` carried two configurations, and each repository deployed its own copy of
download logic. One of them (`auto`) owns a SQLite Durable Object namespace with data.

## Decision
1. One Worker, `ay-edge`, in `apps/edge`. `apps/edge/edge.toml` is the only declaration of its routes, buckets and services; `tools/edge-config.ts`
   renders `wrangler.jsonc` from it and the Worker matches the same table at runtime.
2. Code of this repository is a module of the Worker (`releases`, `assets`, `wasm`; `apps/cf-gateway` and `apps/tools-release` are reused, not copied).
   A script that still owns code or Durable Object state in another repository is a service-mode service: reached through a service binding,
   keeping its cron, queue and class. Nothing of another repository is copied (docs/architecture.md, Independence), and no Durable Object is
   re-declared in a second script. Fusing `auto` is a later one-way step by `transferred_classes` (docs/edge.md).
3. One release layout and one reusable publish workflow (docs/release.md); the Worker serves downloads and the manifest from it.
4. One R2 sync tool, `tools/r2-sync.ts`, with bucket names from `edge.toml`.
5. wasm32 modules are bundled as `CompiledWasm` (a Worker cannot compile bytes at runtime) or served from R2.
6. The live routes move with `tools/edge-cutover.ts`, one route at a time and reversibly, after the Worker is uploaded without routes and checked on
   its workers.dev preview. CI deploys on push to main only when the owner sets `EDGE_DEPLOY`.

## Consequences
- Every public request of these hosts passes through one script: a failure or a plan limit of `ay-edge` is a failure of all of them. The old scripts
  stay deployed during the transition, and `edge-cutover rollback` restores them.
- Routes in `aphrody.com`, `aphrody.app` and the shenron zone change owner; DNS records and zones are not touched. `ay.toml` `[domains]` still marks those
  zones read-only for this repository: the move is authorized by the owner's directive and applied by the owner.
- `fw` joins the Worker when `fw-edge` is deployed or when `fw` exports its handler as a module; its row is disabled until then.
