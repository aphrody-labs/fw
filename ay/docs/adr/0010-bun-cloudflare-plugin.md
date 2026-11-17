# ADR-0010: One Bun plugin that gives every Bun feature a Cloudflare equivalent, and a universal create-cloud-app

Status: proposed, 2026-11-17 (owner request: inventory the estate with proof, plan a complete Bun plugin that reads every
Bun feature and maps it to a Cloudflare equivalent, and set up a universal create-cloud-app). Extends ADR-0006; the
Cloudflare policy of the organization standard and the freeze on `aphrody.com` and `aphrody.app` zones, DNS and routes
still apply: nothing here changes a zone.

## Context
The estate's deployments on Cloudflare are written by hand against the Workers API, each with its own configuration file, so the Bun code the fleet runs and the code the edge runs are two code bases. The state of the account and of the registries (resources, records, counts) is machine- and work-specific: `bun tools/org-inventory.ts` regenerates it into the git-ignored private store (`.private/`, see `tools/local.ts`) and it is not recorded in tracked documents.

The Bun fork documents 120 pages in four sections (runtime, bundler, package manager, test runner). Cloudflare publishes
138 products (`docs/research/cloudflare.toml`). `docs/research/bun-cloudflare.toml` maps each Bun page to the Cloudflare
product that serves the same purpose, with a `parity` value; `bun tools/cloud-map.ts` checks that every page of the
checkout is covered, every product exists in the catalog and every parity is legal. Result: 54 features, 19 `adapter`,
13 `native`, 10 `partial`, 4 `container`, 8 `none`, 0 Bun pages uncovered.

## Decision
One package, `@aphrody/cloud`, in the `bun` repository (`packages/cloud`, ay.toml: "Bun fork and its plugins"). It has
four parts and one rule: **the application source is plain Bun; the target is a build option.**

1. **`analyze`** reads the source with `Bun.Transpiler.scan` and the AST for `Bun.*` and `bun:*` use, and reports for
   each feature the parity and the Cloudflare binding it would need. It is the table of `bun-cloudflare.toml` applied to a
   real project, with file and line. It fails on `none` and unannotated `partial` use when the target is the edge.
2. **Build plugin** (`Bun.build` plugin, `onResolve`/`onLoad`) rewrites `adapter` features: `Bun.serve` to the module's
   default export, `bun:sqlite` to D1, `Bun.S3Client` to the R2 binding, `Bun.sql` to Hyperdrive, `Bun.redis` to KV and a
   Durable Object, `Bun.cron` to `triggers.crons` and `scheduled()`, `Bun.secrets` and `Bun.env` to the Secrets Store and
   `env`, `Bun.WebView` to Browser Run, `Bun.Image` to Cloudflare Images, `Bun.spawn` and `Bun.$` to a Sandbox, websocket
   handlers to a generated Durable Object. It emits the Worker config (`wrangler.jsonc`, or `cloudflare.config.ts` for the
   `cf` CLI) from what the analysis found: the bindings are derived, never typed twice.
3. **Runtime shim** (`@aphrody/cloud/shim`, `preload` in `bunfig.toml`) is the inverse for development: under `bun run` the
   Cloudflare-shaped calls the plugin emits (`env.DB.prepare`, `env.BUCKET.put`) run against bun:sqlite, `Bun.S3Client` and
   the local disk, so one source runs on a laptop, on `<prod-host>` and on the edge.
4. **Conformance suite**: each feature has one test file written with `bun:test`, run twice by `bun test` and
   a workerd run of the same file (the runner is part of the plugin; the command is not designed yet, `bun test` has no `--target` option) (local workerd through the `cf` CLI, real bindings simulated). A feature's `proof` field in
   `bun-cloudflare.toml` is set only when both runs pass; the `parity` claim stands or is corrected from that result.

Keeping the map honest as Bun moves: `tools/cloud-map.ts` already fails when a Bun docs page has no feature; the same check
runs in `tools/mirror`'s gate on the fork, so an upstream merge that adds a Bun API cannot land without a decision.

### create-cloud-app
`bun create @aphrody/cloud-app <name>` (the npm package `@aphrody/create-cloud-app`; `bun create` resolves a scoped name
to `@scope/create-name`). It generates a project that is valid before any edit:
- `src/index.ts` (`Bun.serve` with a route, one stored value, one scheduled job), `src/index.test.ts` (the conformance
  pattern, passing on Bun and on workerd), `cloud.toml` (targets: `bun`, `workers`, `containers`; bindings inferred),
  `bunfig.toml` with the preload, `package.json` scripts `dev`, `test`, `build`, `deploy`.
- `.github/workflows` that call the organization's reusable workflows, so CI, security and release come from the standard.
- `deploy` runs `cf` (never `bunx wrangler`): `workers`-target builds go to Cloudflare; `bun`-target builds go to the fleet
  (`<prod-host>` through the ssh alias of `ay.toml` [hosts]); `containers` builds a `bun build --compile` image for Cloudflare
  Containers. Domain and route are read from `cloud.toml`; the generator refuses `aphrody.com` and `aphrody.app` until the
  owner's decision on the domain roles is recorded.
- Names, scope, org and registry come from the target repository's `ay.toml`-style identity file; nothing is hardcoded.

## Verification status (2026-11-17)
Read in the Cloudflare docs and recorded as `source` in the map: outbound TCP and an inbound `connect()` handler, Hyperdrive on Postgres and MySQL, Durable Object WebSocket hibernation, Secrets Store bindings, `node:fs` virtual file system, Browser Run with a browser binding, Analytics Engine `writeDataPoint`, the Images binding. Everything else in the map, and every `parity` value, is a claim from memory until the conformance suite or a docs page proves it; `bun tools/cloud-map.ts` prints the count. Two statements were looked for and not found: that `eval` and `new Function` are blocked, and that Workers have no UDP API.

## Order of work
1. Conformance harness and `analyze` (read-only, no deploy). Done when a workerd run of the same file (the runner is part of the plugin; the command is not designed yet, `bun test` has no `--target` option) runs one feature file.
2. Adapters in this order, chosen by what production already uses: `Bun.serve`, `Bun.S3Client` (R2), `Bun.env`/`Bun.secrets`,
   `Bun.cron`, then `bun:sqlite` (D1), `Bun.sql` (Hyperdrive), `Bun.redis` (KV and Durable Objects), websockets.
3. `create-cloud-app`, generated from the adapters that are proven, deployed once to a `*.workers.dev` hostname.
4. Port the eleven hand-written Workers one by one to the plugin; each keeps its route until its replacement is proven by a
   DOM or API check, then the old script is deleted.
5. Container and Sandbox features (`spawn`, `$`, `ffi`) last: they cost money and depend on plan limits to be read live.
6. Publication: `@aphrody/cloud` and `@aphrody/create-cloud-app` through npm trusted publishing (one owner trust step per
   new package).

## Consequences
- The parity table is a claim until proven; the first conformance runs will move entries between `adapter`, `partial` and
  `none`. The record is not rewritten: the table in `bun-cloudflare.toml` carries the current state.
- Self-hosting stays possible: the same source targets the fleet, so a Cloudflare limit never blocks a feature.
- D1, KV and Pages are unused today; the plugin introduces them only where an adapter needs them, one resource per purpose.
- `tools/org-inventory.ts` regenerates the account inventory privately; duplicates and orphans of the Cloudflare consolidation are decided from its output, with the domain-role decision still at the end.