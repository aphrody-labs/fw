# Architecture

Updated 2026-11-19. `ay` is the SDK of Aphrody (ADR-0008): C and its derivatives, assembly, Rust, Zig, WebGPU, Python, .NET and
Bun, each where it fits. The complete OS, in pure Rust, is `aphrody-dev/aphrody` and is the source of truth; the dependency
points from `aphrody` to `ay`, never the other way, and `ay` embeds no checkout of another repository (one local checkout
per repository, forks and upstreams as submodules with relative URLs). Independence rule: see below. Decisions are in `docs/adr/` (index in
`docs/adr/README.md`).

## Tree of this repository

```
ay.toml          the only identity and configuration source: orgs, hosts, products, repos, sdk targets, domains
justfile         Pop!_OS / COSMIC-style command runner (check, submodules, dedup, deploy, sessions, sync-host, plan, clean)
submodules.toml  declarative pin manifest for submodules rendered into .gitmodules and 160000 gitlinks
package.json     Bun workspace: private, workspaces = ["packages/*", "apps/*"], no root dependencies
abi/             the ABI spec (ay.abi.toml) and the bindings generated from it (ADR-0011); the C header is generated into corelib/include
corelib/         the SDK corelib: freestanding C23 + assembly + WAT, the C ABI header, WIT world, conformance hosts and proofs
packages/        Bun/TypeScript packages, flat, named @aphrody/ay-<unit>: ai-os (agent kernel, MCP, A2A, LLM discovery, RAG), automate, cf-control-plane (Cloudflare manifest, FinOps, reconcile, cluster configs), chromium, chromium-releases, core, edge-stack (Cloudflare-OS + Vercel Bun SDK), env-kit, ffi, ghidra, gnu, node-compat, omniu, org-mirror, py2bun, wck; the Auto product keeps its own prefix: auto (SDK entry) and auto-client, auto-domain, auto-i18n, auto-native, auto-platform, auto-services, auto-store
os/              OS platform pins and submodules: os/linux (kernel PKGBUILD + Rust userland map), os/nt (aynt), os/aphrody
config/          shell and CLI configs (env.toml, links.toml, bash, pwsh, git, helix, starship, yazi) installed by tools/shells.ts
apps/            programs a person runs, one folder each: chromium-dash, agents (agent harness: runner, GitHub events, plugins), ai-gateway (model gateway overlay), cf-gateway and tools-release (Cloudflare Workers), auto-server, auto-ui, auto-cli, auto-extension, auto-desktop (the Auto product; auto-ui and auto-desktop are standalone Cargo workspaces)
python/          Python workspace (uv, root pyproject.toml): ay_cf_py (Cloudflare SDK and Pydantic control-plane models)
cpp/             C++23 units built with Bazel (root MODULE.bazel): edge_abi
brand/           identity dataset and assets; the token pipeline (Bun) is its generator
tools/           typed Bun scripts for every repeatable flow (features, submodules, paths, private-gate, abi, hosts, disks, listings, absorb, legacy, org-inventory, cloud-map, deploy, claude-sessions, linux-*, rust-map, shells)
docs/            spec, architecture, ADRs, feature matrix, mirrors, standard, science (cited async, recursion, compiler, OS foundations), research, guides
third_party/     upstreams as submodules with relative URLs (DioxusLabs, Pop!_OS/COSMIC, Cloudflare including net, runtime, rust, sdk, agents and python forks, Vercel, media codecs under third_party/media, Browser+VPN, Bun, Nysh, better-auth), one checkout each; submodules.toml also records each fork upstream
crates/          Rust crates of the SDK, directory named like the crate, hyphens only (ay-core/capi/jni, ay-sh, ay-sh-pty, ay-re, ay-ghidra-bun, ay-brand, ay-chromium, ay-dioxus, ay-headless, ay-http-error, ay-patch, ay-softraster, ay-sprite-sheet, ay-svg, ay-boq, ay-deps, ay-ocr-core, ay-ring3-agent; and the AI OS crates: ay-a2a*, ay-agent-*, ay-models, ay-model-client, ay-llama*, ay-infer, ay-embed, ay-rag*, ay-memory, ay-mcp-client, ay-gateway, ay-accounts, ay-config, ay-vault, ay-store, ay-sql, ...)
```

Rules:
- A folder exists only with a working, tested unit; manifests list only members that build.
- Code shared by two repositories lives in a package or in `corelib/`; a product lives in the repository that owns it.
- Upstreams are submodules, never copies. Chromium is fetched with depot_tools `gclient`.
- Nothing generated is committed (logs, reports, dumps), except the artifacts a check compares (`docs/features.md`);
  generators are committed instead.
- Hosts are named by role in `ay.toml`: builds on `build`, tests on `test`, production on `prod`; their paths come from
  `[paths.<role>]`, never from prose.
- Rust units live in `crates/`, built by the root `Cargo.toml` and checked in CI like every other language.

## Private data
Tracked files hold code, schemas, specs and standards. Anything that names this machine, the fleet or the state of the work is kept out
of git: `ay.local.toml` (overrides of `ay.toml`: host aliases, host paths, local checkout hints) and `.private/` (generated reports,
inventories, telemetry, research notes, the work ledger and the plan page; `AY_PRIVATE_DIR` moves it). Documents name hosts by role
(`<build-host>`, `<test-host>`, `<prod-host>` are the aliases of `[hosts]` in `ay.local.toml`). `tools/private-gate.ts` fails `bun run check`
when a tracked file repeats a value of `ay.local.toml`, a user profile or home path, an e-mail address or a public IPv4 address.

## Independence
Every Aphrody repository, `ay` included, builds, tests and releases alone from a clean clone (scheme: `aphrody-labs/.github` `naming/NAMING.md`;
this repository declares its own names in `naming.toml`).
- The sibling checkouts listed under `ay.toml` [repos] (`checkout = "../x"`) are optional local hints. No tool, test or CI job of
  `ay` requires one: a tool that can use a sibling takes it from a flag, an environment variable or the hint, and otherwise skips or
  falls back (`tools/recover.ts` scans only the checkouts that exist; `tools/legacy.ts` skips when the legacy checkout is absent).
- CI never checks out, triggers or waits for another repository.
- Another repository is used through a published version, or a pinned commit that the tool fetches itself into a gitignored cache
  directory inside this repository, always behind a skip or a fallback; never through a path next to this repository.
- The slugs in `[repos]` are metadata (owners of matrix rows, inventory targets), not dependencies.
- Published names are never freed: `naming.toml` [legacy] lists them, new units use `ay-<unit>` (crates `ay-brand`, `ay-chromium`,
  npm `@aphrody/ay-<unit>`).

## Toolchain
One LLVM: clang, clang++ and clang-cl with lld (`ld.lld`, `lld-link`, `wasm-ld`), `llvm-ar`, libc++ and compiler-rt, with
ThinLTO by default and full LTO for releases; Zig and .NET NativeAOT ride the same backend where they can; `bun compile`
(AOT) for typed Bun binaries; tree-sitter is the one parsing layer for code intelligence. Targets (architectures, OSes,
GPUs, ABIs) are the lists in `ay.toml` [sdk]; a target is supported only when CI builds it and a test runs on it. See ADR-0008.

## The ecosystem, by repository
Keys are those of `ay.toml` [repos]; the matrix owner of every feature row is one of them.

| Repository | Role | State 2026-11-18 |
|---|---|---|
| `aphrody-dev/aphrody` | The OS in pure Rust, source of truth; native UI (GPUI, COSMIC, `gpui_app_shell`, `os_craft`); owns `aphrody.com` and `aphrody.app` | Owned by another agent; read-only for this repository |
| `aphrody-dev/ay` | SDK: C ABI, OpenAPI, docs, spec, standards, corelib, libc, design system | This repository |
| `aphrody-dev/anu` | Absorbed into this repository on 2026-11-18 (`os/linux`, `tools/linux-*.ts`, `tools/rust-map.ts`) | Absorbed |
| `aphrody-dev/ai-os` | Absorbed into this repository on 2026-11-18 (`packages/ai-os`, `crates/ay-*`, `apps/agents`, `apps/ai-gateway`); archived | Read only |
| `aphrody-dev/aysh` | Absorbed into this repository on 2026-11-18 (`crates/ay-sh`, `crates/ay-sh-pty`, `config/`, `packages/omniu`, `tools/shells.ts`) | Absorbed |
| `aphrody-dev/ay-tools` | `ay-deps` (`crates/ay-deps`), `ay-re` (`crates/ay-re`, `crates/ay-ghidra-bun`), and `env-kit` (`packages/env-kit`) absorbed; archived | Read only |
| `aphrody-dev/aydroid` | AOSP with Aphrody in place of Android | Empty repository, no content yet |
| `aphrody-dev/aynt` | A free Windows-compatible operating system (fork of ReactOS) | Fork |
| `aphrody-labs/browser` | One browser monorepo: Bun, Servo, Obscura, agent-browser, CEF/Chromium | Receives `ay-chromium` |
| `aphrody-labs/bun` | The Bun fork, the Cloud edge runtime and its plugins | Fork |
| `aphrody-labs/craft-anything` | Replication engine, consumed by `aphrody` as `vendor/craft-anything` | Fork |
| `aphrody-labs/open` | Open ecosystem (opencode, opentui, models.dev) | Own agent |
| `aphrody-labs/auto` | Absorbed into this repository on 2026-11-19 (`apps/auto-*`, `packages/auto*`, `crates/auto-*`, `docs/auto`, `tools/auto-*`); archived | Read only |
| `aphrody-labs/ay-cf` | Absorbed into this repository on 2026-11-19 (`packages/cf-control-plane`, `apps/cf-gateway`, `apps/tools-release`, `tools/cf-*`, `python/`, `cpp/`, `third_party/cloudflare`); archived | Read only |
| `aphrody-labs/ay-media` | Absorbed into this repository on 2026-11-19 (`crates/ay-media-*`, `third_party/media`); archived | Read only |
| `aphrody-labs/aphrody-m3` | Material 3 UI packages, icons, fonts | Consumes the `ay` token pipeline |
| `aphrody-labs/nie` | Football games built as craft jobs | Own agent |
| `rose-griffon/rosegriffon` | Rose Griffon monorepo, moved from `aphrody-labs` to its own organization | Own agent |
| `aphrody-labs/.github` | The organization standard 1.0.0, defaults, reusable CI, profile | See `docs/standard.md` |
| `aphrody-labs/legacy` | The old monorepo, preserved archive, workflows disabled | Read only |

## Products and their stacks
| Product | Stack | Owner repository | ADR |
|---|---|---|---|
| os | Rust OS; LFS stage-0 and Nix packages for the Linux flavour; kernel.org + CachyOS patches; Wayland desktop; Waydroid and `aydroid` | `aphrody` | ADR-4 |
| browser | Chromium through CEF with our UI, agent panel and WebMCP | `browser` | ADR-3 |
| search | Rust crawler, index and ranker; Bun front end; AI mode over the OpenAI-compatible model API | `aphrody` | ADR-5 |
| cloud | Bun edge runtime with a Workers-compatible API; deploys, domains, TLS, storage, logs | `aphrody` on `bun` | ADR-6 |
| craft | Engine `craft-anything`, its CLI and MCP server; native GUI in `aphrody` | `craft-anything` | ADR-1 |
| brand | Character dataset, one Bun token pipeline, Google Sans Flex, Material 3 | `ay` | ADR-1 |

ADR-3 to ADR-6 describe products that moved out of this repository; they stay here as the decision record until each
owner repository carries its own copy.

## Design system and tokens
One Bun pipeline owned by `ay` generates the tokens from the brand seed and the Material 3 rules: DTCG JSON, CSS custom
properties, a Tailwind v4 preset, a Jetpack Compose Material 3 colour scheme (`brand/tokens/compose`, `bun tools/tokens-compose.ts`) and a C header (target; today `brand/tokens` is colour only and `aphrody-m3` holds
shape, type, motion, elevation and state). Figma is the visual source and is mirrored to code through variables and Code
Connect (`docs/mirrors.toml`). `aphrody-m3` and `aphrody` consume the output and do not regenerate it.

## Native UI lives in aphrody-dev/aphrody
GPUI, gpui-kit, COSMIC and iced, the desktop shell and the Craft native UI belong to `aphrody-dev/aphrody` (a Zed fork).
No GPUI or COSMIC code is written here. The browser and the web products do not depend on it.
