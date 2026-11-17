# Architecture

Updated 2026-11-17. `ay` is the SDK of Aphrody (ADR-0008): C and its derivatives, assembly, Rust, Zig, WebGPU, Python, .NET and
Bun, each where it fits. The complete OS, in pure Rust, is `aphrody-dev/aphrody` and is the source of truth; the dependency
points from `aphrody` to `ay`, never the other way, and `ay` embeds no checkout of another repository (one local checkout
per repository, forks and upstreams as submodules with relative URLs). Independence rule: see below. Decisions are in `docs/adr/` (index in
`docs/adr/README.md`).

## Tree of this repository

```
ay.toml          the only identity and configuration source: orgs, hosts, products, repos, sdk targets, domains
package.json     Bun workspace: private, workspaces = ["packages/*", "apps/*"], no root dependencies
corelib/         the SDK corelib: freestanding C23 + assembly + WAT, the C ABI header, WIT world, conformance hosts and proofs
packages/        Bun/TypeScript packages, flat, named @aphrody/<name>: chromium, chromium-releases, org-mirror
apps/            programs a person runs, one folder each: chromium-dash
brand/           identity dataset and assets; the token pipeline (Bun) is its generator
tools/           typed Bun scripts for every repeatable flow (features, paths, hosts, disks, listings, absorb, legacy)
docs/            spec, architecture, ADRs, feature matrix, mirrors, standard, research, guides
third_party/     upstreams as submodules with relative URLs, one checkout each
crates/          Rust crates of the SDK (ay-brand, ay-chromium)
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

| Repository | Role | State 2026-11-17 |
|---|---|---|
| `aphrody-dev/aphrody` | The OS in pure Rust, source of truth; native UI (GPUI, COSMIC, `gpui_app_shell`, `os_craft`); owns `aphrody.com` and `aphrody.app` | Owned by another agent; read-only for this repository |
| `aphrody-dev/ay` | SDK: C ABI, OpenAPI, docs, spec, standards, corelib, libc, design system | This repository |
| `aphrody-dev/anu` | Aphrody's Not Unix: uutils, GNU, Linux on the Arch family, Nix; no dependency on any other Aphrody repository | Own repository, own agent |
| `aphrody-dev/ai-os` | Agent kernel, relay (MCP, A2A), model, inference and RAG, extracted from `legacy` by `tools/extract.ts` | Own repository, own agent |
| `aphrody-dev/aydroid` | AOSP with Aphrody in place of Android | Empty repository, no content yet |
| `aphrody-dev/aynt` | A free Windows-compatible operating system (fork of LiberNT) | Fork |
| `aphrody-labs/browser` | One browser monorepo: Bun, Servo, Obscura, agent-browser, CEF/Chromium | Receives `ay-chromium` |
| `aphrody-labs/bun` | The Bun fork, the Cloud edge runtime and its plugins | Fork |
| `aphrody-labs/craft-anything` | Replication engine, consumed by `aphrody` as `vendor/craft-anything` | Fork |
| `aphrody-labs/open`, `auto` | Open ecosystem (opencode, opentui, models.dev) and workflow automation | Own agents |
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
properties, a Tailwind v4 preset and a C header (target; today `brand/tokens` is colour only and `aphrody-m3` holds
shape, type, motion, elevation and state). Figma is the visual source and is mirrored to code through variables and Code
Connect (`docs/mirrors.toml`). `aphrody-m3` and `aphrody` consume the output and do not regenerate it.

## Native UI lives in aphrody-dev/aphrody
GPUI, gpui-kit, COSMIC and iced, the desktop shell and the Craft native UI belong to `aphrody-dev/aphrody` (a Zed fork).
No GPUI or COSMIC code is written here. The browser and the web products do not depend on it.
