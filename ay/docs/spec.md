# Aphrody: product specification

## Vision
Aphrody crafts four products and the brand that ties them together:

| Product | Replica of | What "done" means |
|---|---|---|
| **Aphrody OS** | The Googlebook experience (googlebook.google) | A bootable, installable Linux system whose desktop offers every Googlebook feature in `docs/features.toml`, AI-first, with our own model layer |
| **Aphrody Browser** | Chromium-based browsers | A Chromium build with our UI, the agent panel, WebMCP and our search as the default engine, installable on Linux and Windows |
| **Aphrody Search** | google.com | A search home and results page matching google.com feature for feature, an AI mode, image search and file Q&A, served from our own crawler and index |
| **Aphrody Cloud** | Cloudflare Workers + Vercel | A Bun edge runtime with a Workers-compatible API, git-push deploys, previews, domains and TLS, storage, logs and a dashboard |
| **Craft Anything** | storytold/craft-anything | The engine that studies a reference and rebuilds it; it drives the replication of every product above |

Any language does the job it suits: Rust, C, C++, Go, Bun/TypeScript, .NET, Python. The core is Rust plus the Bun fork (`aphrody-labs/bun`), kept a pure JavaScript runtime; craft-anything is its own fork (`aphrody-labs/craft-anything`, with `aphrody-labs/craft-libs`) consumed as a submodule.

## Quality bar
- No prototypes, scaffolds or placeholders: a directory exists only with a working, tested unit.
- A feature is done only with a proof recorded in the feature matrix: a test id, a screenshot or a log. CI refuses "done" without one.
- Standards are conformance-tested: W3C/WHATWG via web-platform-tests, accessibility at WCAG 2.2 AA, Core Web Vitals via Lighthouse, network protocols (DNS, TLS 1.3, IPv4/6, HTTP/1.1-2-3, WebSockets, WebRTC) via their test suites.
- Agents and tools speak the open protocols: an OpenAI-compatible API, OpenAPI, MCP and WebMCP, ACP, A2A.
- Code follows the Google style guides, rustfmt/clippy, and the Google eng-practices review criteria; docs follow the Google developer documentation style guide.
- One LLVM toolchain with ThinLTO across C, C++ and Rust; Bun binaries are AOT-compiled where typed.

## Non-goals
- Copying any upstream's directory layout (the previous monorepo mirrored AOSP and broke).
- Google-scale search parity: the index grows from seeded domains and its size and freshness are measured, never claimed.
- Shipping Google's proprietary services or images; Android apps run through Waydroid, AI runs on our model layer.

## Order of delivery
`brand` → `craft` → `search` → `cloud` → `browser` → `os`. Each product uses the ones before it.

## Known blockers
- The browser, if a full Chromium build is ever needed, has room: fr1's build volume `/srv/xbuild` (XFS on LVM, 805 GB) has about 740 GB free, above Chromium's 100 GB floor; fr1's root filesystem is the small one. `bun tools/disks.ts` is the measure.
- Hardware-only OS features (biometric unlock, TPM secure boot, phone continuity, the status light) need a real laptop and phone for their proofs.
- Inference capacity on the fleet is unmeasured; AI features use a hosted fallback behind the OpenAI-compatible API until it is.
