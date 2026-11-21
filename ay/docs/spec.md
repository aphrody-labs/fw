# Aphrody: SDK specification

Governed by ADR-0008. This repository, `aphrody-labs/ay`, is the **SDK** of Aphrody:
C ABI, OpenAPI, docs, spec, standards, corelib, libc and the design system, in C and its derivatives, assembly, Rust, Zig,
WebGPU, Python, .NET and Bun, each where it fits. The complete OS, in pure Rust, is `aphrody-labs/aphrody` and is the
source of truth; this SDK follows it. `ay` may depend on `fw` (lower in the stack) through sibling paths, never the reverse. Names and owners come from `ay.toml`, hosts from the git-ignored `ay.local.toml`.

## Vision
Aphrody crafts four products and the brand that ties them together. Each row of the feature matrix
(`docs/features.toml`) names the repository that builds it (`owner`).

| Product | Replica of | Built in | What "done" means |
|---|---|---|---|
| **Aphrody OS** | The Googlebook experience (googlebook.google) | `aphrody` (Rust), `fw-nt` (NT) | A bootable, installable system whose desktop offers every Googlebook feature in the matrix, AI-first, with our own model layer (in `fw`) |
| **Aphrody Browser** | Chromium-based browsers | `browser` | A browser with our UI, the agent panel, WebMCP and our search as the default engine, installable on Linux and Windows |
| **Aphrody Search** | google.com | `aphrody` | A search home and results page matching google.com feature for feature, an AI mode, image search and file Q&A, served from our own crawler and index |
| **Aphrody Cloud** | Cloudflare Workers + Vercel | `aphrody` on the Bun fork (`bun`) | A Bun edge runtime with a Workers-compatible API, git-push deploys, previews, domains and TLS, storage, logs and a dashboard |
| **Craft Anything** | storytold/craft-anything | `craft-anything` | The engine that studies a reference and rebuilds it; it drives the replication of every product above |
| **Brand and design system** | design.google, Material 3 | `ay` (this repository) | One token source (`brand/tokens`: Material 3 colour JSON and CSS, a Jetpack Compose theme) |

Any language does the job it suits. In this repository: C and its derivatives, assembly, Rust, Zig, WebGPU (WGSL), Python,
.NET and Bun (ADR-0008). The ecosystem, by repository, is in
`docs/architecture.md`.

## What this repository owns
- **C ABI**: the stable boundary between every language, headers generated from one spec; the freestanding C23
  `corelib` on WebAssembly, bare-metal x86-64 and native (ELF, PE and MSVC ABI), with conformance hosts and proofs.
- **OpenAPI** for every HTTP service and the WIT world for WebAssembly components (specs to write: `docs/api/`).
- **Design system**: Material 3 and M3 Expressive tokens, Google Sans Flex and Material Symbols (self-hosted), contrast,
  motion and state rules, Figma file and Code Connect mappings (`docs/design/`, to write from the one token source).
- **Standards**: the organization standard lives in `aphrody-labs/.github`; `docs/standard.md` links it and says how this
  repository applies it.
- **Feature matrix and decisions**: `docs/features.toml`, `docs/adr/`, the design package mirrored to Drive and Figma
  (`docs/mirrors.toml`).
- **Cloud and edge**: `apps/edge`, `apps/cf-gateway`, `apps/tools-release`, `packages/cf-control-plane`, `packages/edge-stack`, `crates/ay-cf`, `third_party/cloudflare`, `tools/cf-*.ts`.
- **Environment and shells**: `config/`, `packages/omniu`, `packages/env-kit`, `tools/shells.ts` (the shell engines, PTY, agent kernel and model layer live in `fw` and `fw-nt`).
- **Reverse engineering**: `crates/ay-re`, `crates/ay-ghidra-bun`, `packages/ghidra`, `packages/py2bun`.
- **Workspace**: the registry `ay.toml` [repos], `tools/workspace.ts`, `workspace.just`, the organization mirror (`packages/org-mirror`).
- Moved out: the Chromium release datasets, dashboards and CEF embedding to `browser`; the Linux platform and the agent kernel to `fw`.

## Quality bar
- No prototypes, scaffolds or placeholders: a directory exists only with a working, tested unit.
- A feature is done only with a proof recorded in the feature matrix: a test id, a screenshot or a log. CI refuses "done" without one.
- Standards are conformance-tested: W3C/WHATWG via web-platform-tests, accessibility at WCAG 2.2 AA, Core Web Vitals via Lighthouse, network protocols (DNS, TLS 1.3, IPv4/6, HTTP/1.1-2-3, WebSockets, WebRTC) via their test suites.
- Agents and tools speak the open protocols: an OpenAI-compatible API, OpenAPI, MCP and WebMCP, ACP, A2A.
- Code follows the Google style guides and the Google eng-practices review criteria; docs follow the Google developer documentation style guide and the Aphrody Labs standard (README, Markdown and MDX rules).
- One LLVM toolchain with ThinLTO across C, C++ and the targets in `ay.toml` [sdk]; Bun binaries are AOT-compiled where typed.

## Non-goals
- Copying any upstream's directory layout (the previous monorepo mirrored AOSP and broke).
- Google-scale search parity: the index grows from seeded domains and its size and freshness are measured, never claimed.
- Shipping Google's proprietary services or images; Android apps run through Waydroid, AI runs on our model layer.
- Writing the OS's GPUI or COSMIC desktop code here (it belongs to `aphrody`), or owning the domains `aphrody.com` and
  `aphrody.app` (they belong to the aphrody monorepo; DNS and per-domain roles are decided there).

## Order of delivery
`sdk` (C ABI, corelib, tokens) → `craft` → `search` → `cloud` → `browser` → `os`. Each product uses the ones before it;
the SDK is first because every other repository consumes it.

## Known blockers
- Hosts and their volumes are read from `ay.local.toml` [hosts] and [paths.<role>]; `bun tools/disks.ts` measures them. A full Chromium
  build needs 100 GB free on the build host's work volume; that is only needed if the browser ever leaves CEF.
- Hardware-only OS features (biometric unlock, TPM secure boot, phone continuity, the status light) need a real laptop and phone for their proofs.
- Inference capacity on the fleet is unmeasured; AI features use a hosted fallback behind the OpenAI-compatible API until it is.
- Spec debt in this repository: component specs, motion and icon policy, adaptive rules, versioning, screenshot
  regression against Figma, the OpenAPI and C header specs, the WIT document and the release process are written
  under `docs/design/` and `docs/api/` as the token pipeline lands.
