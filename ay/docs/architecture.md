# Architecture

Updated 2026-11-20. `ay` is the SDK of Aphrody (ADR-0008): C and its derivatives, assembly, Rust, Zig, WebGPU, Python, .NET and
Bun, each where it fits. The workspace is one root with sibling repositories, in stack order: `fw` (toolchain and agent runtime), `aphrody`
(OS and desktop), `ay` (this SDK), `nie` (games and engine), `shenron`; `browser`, `wiki`, `fw-nt`, `bun`, `craft-anything` and `standard`
(`aphrody-labs/.github`) are satellites (keys of `ay.toml` [repos]). `ay` may depend on `fw`, which is lower in the stack, through
sibling paths; `fw` never depends on `ay`. Independence rule: see below. Decisions are in `docs/adr/` (index in `docs/adr/README.md`).

Workspace role: `ay.toml` [repos] is the registry of the live repositories, `tools/workspace.ts` and `workspace.just` (imported by
the root `justfile`) report the status of the sibling checkouts under one root, audit them against the registry, pull them and run recipes in them. Each upstream fork is checked out by exactly one
repository: `fw-nt` owns the Windows/NT forks (`forks.toml`), `aphrody` the UI/GPU forks (pinned Cargo git dependencies), `ay` the
forks of `submodules.toml`.

## Tree of this repository

```
ay.toml          the only identity and configuration source: orgs, products, repos, retired repos, sdk targets, domains
justfile         SDK recipes (check, submodules, dedup, deploy, cf-*, py-check, cpp-check, sessions, sync-host, plan, clean); imports workspace.just
workspace.just   workspace-wide recipes (status, audit, manifests, pull, tools, cache, in), driven by tools/workspace.ts
submodules.toml  declarative pin manifest for submodules rendered into .gitmodules and 160000 gitlinks
package.json     Bun workspace: private, workspaces = ["packages/*", "packages/env-kit/*", "apps/*"]
abi/             the ABI spec (ay.abi.toml) and the bindings generated from it (ADR-0011); the C header is generated into corelib/include
corelib/         the SDK corelib: freestanding C23 + assembly + WAT, the C ABI header, WIT world, conformance hosts and proofs
packages/        Bun/TypeScript packages, flat, named @aphrody/ay-<unit>: automate, cf-control-plane (Cloudflare manifest, FinOps, reconcile, cluster configs), core, edge-stack (Cloudflare-OS + Vercel Bun SDK), env-kit, ffi, ghidra, node-compat, omniu, org-mirror, py2bun, wck; the Auto product keeps its own prefix: auto (SDK entry) and auto-client, auto-domain, auto-i18n, auto-native, auto-platform, auto-services, auto-store
config/          shell and CLI configs (env.toml, links.toml, bash, pwsh, git, helix, starship, yazi) installed by tools/shells.ts
apps/            programs a person runs, one folder each: ay (the CLI), edge (the unified Worker), cf-gateway and tools-release (Cloudflare Workers), auto-server, auto-ui, auto-cli, auto-extension, auto-desktop (the Auto product; auto-ui and auto-desktop are standalone Cargo workspaces)
python/          Python workspace (uv, root pyproject.toml, uv.lock): ay_cf_py (Cloudflare SDK and Pydantic control-plane models)
cpp/             C++23 units built with Bazel (root MODULE.bazel): edge_abi
kotlin/          Kotlin workspace (Gradle wrapper, AGP, version catalog): aphrody-theme, the Aphrody Compose theme compiled in place from brand/tokens/compose and published as com.aphrody:aphrody-theme (group from ay.toml [domains], version from Cargo.toml) to the GitHub Packages Maven registry; gate `bun tools/compose-check.ts` (runs on the build host)
brand/           identity dataset and assets; the token pipeline (Bun) is its generator
locales/         message catalogs (TSV) of the Auto product, read by packages/auto-i18n
tools/           typed Bun scripts for every repeatable flow (features, submodules, workspace, paths, private-gate, abi, hosts, disks, listings, legacy, org-inventory, cloud-map, deploy, claude-sessions, shells, map)
docs/            spec, architecture, ADRs, feature matrix, mirrors, standard, science (cited async, recursion, compiler, OS foundations), research, guides
third_party/     upstreams as submodules with relative URLs (`../<name>.git` in the same organization): DioxusLabs, COSMIC toolkit, Cloudflare (net, runtime, rust, sdk, agents and python forks), Vercel, media codecs under third_party/media, Bun, better-auth, one checkout each; submodules.toml also records each fork upstream. Forks owned by another repository (niubash, rubash, ruline, wgpu in `fw-nt`; taffy, accesskit, cosmic-text, mullvad, yt-dlp in `aphrody`) are not checked out here
crates/          Rust crates of the SDK, directory named like the crate, hyphens only: `ay-core` (facts engine, C ABI capi and JNI), `ay-re` (reverse-engineering suite), `ay-ghidra-bun` (cdylib), `ay-vault` (cdylib C ABI of the vault), `ay-build` (deps, siso), `ay-cf` (assets, edge), `ay-media` (luma, hls), `ay-design` (brand, dioxus, svg, raster, glyph, symbols, softraster, sprites), `ay-accounts` (OS keyring OAuth), `ay-app-protocol`, `aphrody-auto` in `auto-sdk` and `auto-ffi` (the Auto product). The HTTP stack, configuration, secrets, agents, models, RAG and shells live in `fw`. A merged crate has no default features: a feature names a capability, `full` turns them all on, and the crate depends on itself with `full` in `[dev-dependencies]` so `cargo test` covers every feature
```

Rules:
- A folder exists only with a working, tested unit; manifests list only members that build.
- Code shared by two repositories lives in a package or in `corelib/`; a product lives in the repository that owns it.
- Upstreams are submodules, never copies, and one repository of the workspace checks out each fork. Chromium is fetched with depot_tools `gclient` by `browser`.
- Nothing generated is committed (logs, reports, dumps), except the artifacts a check compares (`docs/features.md`);
  generators are committed instead.
- Hosts are named by role: builds on `build`, tests on `test`, production on `prod`; their aliases and paths come from
  `ay.local.toml` `[hosts]` and `[paths.<role>]`, never from prose.
- Rust units live in `crates/`, built by the root `Cargo.toml` and checked in CI like every other language.

## Private data
Tracked files hold code, schemas, specs and standards. Anything that names this machine, the fleet or the state of the work is kept out
of git: `ay.local.toml` (overrides of `ay.toml`: host aliases, host paths, local checkout hints) and `.private/` (generated reports,
inventories, telemetry, research notes, the work ledger and the plan page; `AY_PRIVATE_DIR` moves it). Documents name hosts by role
(`<build-host>`, `<test-host>`, `<prod-host>` are the aliases of `[hosts]` in `ay.local.toml`). `tools/private-gate.ts` fails `bun run check`
when a tracked file repeats a value of `ay.local.toml`, a user profile or home path, an e-mail address or a public IPv4 address.

## Independence
Dependencies point down the stack (scheme: `aphrody-labs/.github` `naming/NAMING.md`; this repository declares its own names in `naming.toml`):
`ay` may depend on `fw`, which is lower, through sibling paths (`../fw`, resolved from the workspace root, or the `checkout` hint of
`ay.local.toml`); `fw` never depends on `ay`. Toward every other repository `ay` stays independent.
- A tool or test of `ay` that can use another sibling takes it from a flag, an environment variable or the hint, and otherwise skips or
  falls back (`tools/recover.ts` scans only the checkouts that exist; `tools/legacy.ts` skips when the legacy checkout is absent).
- CI checks out `fw` as a sibling when a job builds code that depends on it; it never triggers or waits for another repository.
- Any other repository is used through a published version, or a pinned commit that the tool fetches itself into a gitignored cache
  directory inside this repository, always behind a skip or a fallback.
- The slugs in `[repos]` are the workspace registry (owners of matrix rows, inventory targets, `tools/workspace.ts`).
- Published names are never freed: `naming.toml` [legacy] lists them, new units use `ay-<unit>` (crates `ay-design`, `ay-media`,
  npm `@aphrody/ay-<unit>`).

## Toolchain
One LLVM: clang, clang++ and clang-cl with lld (`ld.lld`, `lld-link`, `wasm-ld`), `llvm-ar`, libc++ and compiler-rt, with
ThinLTO by default and full LTO for releases; Zig and .NET NativeAOT ride the same backend where they can; `bun compile`
(AOT) for typed Bun binaries; tree-sitter is the one parsing layer for code intelligence. Targets (architectures, OSes,
GPUs, ABIs) are the lists in `ay.toml` [sdk]; a target is supported only when CI builds it and a test runs on it. See ADR-0008.

## The ecosystem, by repository
Keys are those of `ay.toml` [repos]; the matrix owner of every feature row is one of them.

| Repository | Role |
|---|---|
| `aphrody-labs/fw` | Toolchain and agent runtime: Rust CLI, MCP/ACP/A2A, inference, shells, the agent plugin; lower in the stack than `ay` |
| `aphrody-labs/aphrody` | The OS and desktop (Zed fork, GPUI, compositor, installer); owns `aphrody.com`, `aphrody.app` and the UI/GPU forks |
| `aphrody-labs/ay` | This repository: SDK (C ABI, corelib, OpenAPI, design tokens), cloud/edge, workspace orchestration |
| `aphrody-labs/nie` | Game engine and recompiler (Inazuma Eleven: Victory Road first) |
| `aphrody-labs/shenron` | dragonballfr.com, Bun only; owns `data/` and the `@aphrody/shenron-*` packages |
| `aphrody-labs/wiki` | Rust workspace of `shenron` (koharu, dragon-pixel, shenron-* crates) |
| `aphrody-labs/browser` | One browser workspace: Blink headless, Servo/Obscura lite engine, agent-browser, the Chromium release datasets and dashboards |
| `aphrody-labs/fw-nt` | NT operating system around `fw`; owns the Windows/NT forks |
| `aphrody-labs/bun` | The Bun fork, the Cloud edge runtime and its plugins |
| `aphrody-labs/craft-anything` | Replication engine, consumed by `aphrody` as `vendor/craft-anything` |
| `aphrody-labs/.github` | The organization standard 1.0.0, defaults, reusable CI, profile (see `docs/standard.md`) |

Absorbed here and archived: `aphrody-labs/auto` (2026-11-19: `apps/auto-*`, `packages/auto*`, `crates/auto-*`, `docs/auto`, `tools/auto-*`),
`aphrody-labs/ay-cf` (2026-11-19: `packages/cf-control-plane`, `apps/cf-gateway`, `apps/tools-release`, `tools/cf-*`, `python/`, `cpp/`,
`third_party/cloudflare`), `aphrody-labs/ay-media` (2026-11-19: `crates/ay-media`, `third_party/media`). Repositories that left the live stack
(`anu`, `ai-os`, `aydroid`, `open`, `aphrody-m3`, `claude`, `aynt`, `rosegriffon`) are the `[retired]` table of `ay.toml`, each with its reason; `aphrody-labs/legacy` is the
preserved archive of the old monorepo.

## Products and their stacks
| Product | Stack | Owner repository | ADR |
|---|---|---|---|
| os | Rust OS; LFS stage-0 and Nix packages for the Linux flavour; kernel.org + CachyOS patches; Wayland desktop; Waydroid | `aphrody` | ADR-4 |
| browser | Chromium through CEF with our UI, agent panel and WebMCP | `browser` | ADR-3 |
| search | Rust crawler, index and ranker; Bun front end; AI mode over the OpenAI-compatible model API | `aphrody` | ADR-5 |
| cloud | Bun edge runtime with a Workers-compatible API; deploys, domains, TLS, storage, logs | `aphrody` on `bun` | ADR-6 |
| craft | Engine `craft-anything`, its CLI and MCP server; native GUI in `aphrody` | `craft-anything` | ADR-1 |
| brand | Character dataset, one Bun token pipeline, Google Sans Flex, Material 3 | `ay` | ADR-1 |

ADR-3 to ADR-6 describe products that moved out of this repository; they stay here as the decision record until each
owner repository carries its own copy.

## Design system and tokens
`brand/tokens/` holds the Material 3 colour tokens: `m3-tokens.json` (light and dark roles), `m3-tokens.css` (custom properties) and
`m3-symbols.css`. `bun tools/tokens-compose.ts` renders `brand/tokens/compose` (Compose Material 3 colour schemes and `AphrodyTheme` on
`MaterialExpressiveTheme`), checked by `bun run check`; `kotlin/aphrody-theme` compiles that file in place into an Android library, so other
repositories depend on `com.aphrody:aphrody-theme` and never copy it. Shape, type, motion, elevation and state are not in this tree: the Material 3
packages live in `shenron` and `aphrody` (`ay.toml` [retired] `aphrody-m3`). The Figma variables equal `m3-tokens.json` (`docs/mirrors.toml` [figma]).

## Native UI lives in aphrody-labs/aphrody
GPUI, gpui-kit, COSMIC and iced, the desktop shell and the Craft native UI belong to `aphrody-labs/aphrody` (a Zed fork).
No GPUI or COSMIC code is written here. The browser and the web products do not depend on it.
