# Roadmap: one polyglot monorepo and a shipped OS (started 2026-11-21)

Owner request (2026-11-21), verbatim intent:
- Merge ay, aphrody and fw into one refactored monorepo with the best skills, research and examples.
- Ship a real final product with releases and tags: site, Windows, Android, CLI, MCP, a complete HTTP server, the UI framework and design system, subdomains, DNS, tunnels, buckets, Postgres, Redis, Workers, auto-deploy, auto-scale and the full Vercel feature set in Rust.
- Run full CachyOS on one VPS with a unified Rust userland, the optimised Bun fork, and a reproducible wasm/WebGPU compiler and OS tooling.
- No prototypes, no shortcuts.

Every phase ends on an **acceptance gate**. A gate is a job independent of the implementer that rebuilds and tests the pushed SHA and writes the verdict (see skill `formal-verification`, `references/lessons-for-agents.md`). An agent's own report is never acceptance. At most 3 background agents run at once; extra work waits as a handoff in `docs/agents/queue/`.

## Measured starting point (2026-11-21)

| Repo | Size | History | Toolchain |
|---|---|---|---|
| aphrody (Zed fork, GPUI, desktop) | ~343 workspace crates | 41,037 commits, 466 MiB pack | Rust 1.100.0 |
| ay (SDK, Bun tooling, cloud/edge) | 29 packages and apps | 245 commits, 24 MiB | Rust 1.100.0 |
| fw (core: toolchain, agents, CLI, MCP) | ~40 crates (+ fw-browser-* merged, not yet members) | 261 commits, 71 MiB | Rust 1.100.0 |

Infrastructure: fr1 (Haswell, 12 vCPU, 45 GB, `$WORK=/srv/xbuild` 805 GB, NativeLink), prd (EPYC Zen 4, 8 vCPU, 31 GB, CachyOS kernel, production being moved to Cloudflare), Cloudflare (Workers, R2, tunnels), gcloud (12 projects, one open billing account, credits not yet read), a PC with RTX 4070 (i7-13700F, 32 GB). fr2 is out of service.

## Decisions

1. **The monorepo is `aphrody-labs/fw`.** fw is the core, and the other repos merge into it with full history (`git filter-repo --to-subdirectory-filter`, then merge with `--allow-unrelated-histories`). Each source repo becomes a read-only redirect only after the gate passes.
2. **Layout:**
   - `crates/`: every first-party Rust crate in one Cargo workspace.
   - `packages/`: Bun/TS packages in one Bun workspace.
   - `apps/`: products: fw CLI, desktop, web, android, windows, site.
   - `os/`: CachyOS image, userland, kernel config.
   - `infra/`: OpenTofu for Cloudflare, GCP, OVH, Hostinger, DNS and tunnels.
   - `third_party/`: forks as relative-URL submodules.
   - `plugins/`: the agent plugin.
   - `docs/`, `tools/`, `xtask/`.
3. **Build:** native Cargo and Bun builds stay. Orchestration, affected-only runs and caching go through moon (v2.6.5, Rust, Bun and Rust toolchains). Its remote cache talks the Bazel RE API to NativeLink on fr1, and rustc uses sccache-dist across fr1 and prd.
   - Bazel 9.3.3 was rejected for now: it would mean rewriting about 383 crates' builds into rules_rust/rules_js. Re-evaluate if moon plus NativeLink cannot reach the gate numbers.
   - Gradle stays for Kotlin/Android; uv for the remaining Python, which is to be ported.
4. **Layering is enforced, not trusted.** The old repo order (fw ← aphrody ← ay) becomes a dependency rule that CI checks inside the monorepo (`cargo deny` bans plus a layer check in `xtask`).
5. **Infra as code:** OpenTofu with SOPS+age secrets, restic backups to R2, VictoriaMetrics/Logs (see `docs/research/cluster-stack-2026-11-21.md`).
6. **Legacy removal is continuous:** the 170 Python and 77 JS files in fw (scc, 2026-11-21) are ported to Rust or Bun or deleted, and npm/npx/node/ffmpeg leave every path.

## Phases and gates

### Phase 0: stabilise (in progress)

- Collect all agent handoffs; 11 of 13 are in.
- Single checkout per repo at `$WORK/src/<repo>`, a shared target dir, sccache-dist across fr1 and prd.
- Clean fr1's root and `$WORK` of pushed, idle duplicates.
- Land or salvage every uncommitted edit in the PC checkouts (aphrody has many from other sessions).

**Gate:**
- `git status` is clean on every canonical checkout;
- `df` shows fr1's root below 70%;
- fw release build time is measured, first on fr1 alone, then on fr1 + prd.

### Phase 1: monorepo merge

- Dry-run the merge in a scratch clone on `$WORK`: filter-repo ay and aphrody into subdirectories, merge into fw, unify `[workspace.dependencies]` and resolve version conflicts, then one `Cargo.lock` and one `bun.lock`.
- Wire moon (projects, tasks, affected, remote cache), the layer check and CI (one pipeline, affected-only).
- Then perform the real merge on main and redirect the old repos.

**Gate:**
- `cargo check --workspace` and `cargo nextest run --workspace` pass on the merged SHA;
- `bun check`, `bun lint` and `bun test` pass;
- the web wasm build passes (`--slim --webgpu-only`);
- the desktop builds for Linux and Windows;
- no history lost: the commit count is at least the sum of the sources;
- moon's cache hit rate on a no-op rebuild is ≥ 95%.

### Phase 2: release train (product, not prototype)

- **Versioning:** one version scheme with tags `vYYYY.MM.N` and signed release artifacts.
- **fw CLI and MCP server:**
  - Windows, Linux and macOS binaries;
  - `fw self-update` verified end to end.
- **Desktop (aphrody):**
  - Windows installer, Linux packages;
  - the web desktop at web.aphrody.com, already auto-deployed per main push.
- **Android:**
  - an APK/AAB built from the Compose Multiplatform app (handoff `android-lab`);
  - an F-Droid repo;
  - Play upload prepared.
- **HTTP server and "Vercel in Rust"** (fw-http and fw-edge):
  - deploy previews per commit, serverless functions, ISR/edge cache;
  - custom domains and subdomains, DNS, TLS, tunnels;
  - R2 buckets, Postgres (Cloud SQL or Hyperdrive), Redis/Valkey cache, Workers;
  - auto-deploy on push, autoscale through Cloudflare plus gcloud.
- **UI framework and design system:** GPUI, gpui-kit and fw-m3 published with docs and examples, plus a site generated from the code (the aphrody `site/` generator).

**Gate:** each artifact is installed or deployed and proven by content: a rendered page, an API payload, an installed app screenshot. A bare HTTP status proves nothing.

### Phase 3: reproducible OS

- A CachyOS-based image (x86-64-v3 and v4) built reproducibly from the monorepo's `os/`.
- One VPS runs the unified Rust userland: fw-uutils and the `fw` applets.
- The Bun fork is installed as the system runtime (merged upstream at `6b1aa57b99`, WebP effort `9a07424a0f`).
- The wasm and WebGPU toolchain is pinned (`configs/claude/toolchain.toml`, fw `71bf4153`).

**Gate:**
- two independent builds produce byte-identical images;
- the VPS boots the image and serves the release train's services;
- the gpu-bench and raster results are published.

## Queue mapped to phases (handoffs in `docs/agents/queue/`)

- **Phase 0:** fleet (CachyOS build hosts, prod → Cloudflare), fw optimisation, gcloud credits.
- **Phase 1:** `fw depot` (depot_tools 160 in Rust; it moves `ay-build` into fw), browser merge wiring, wasm toolchain optimisation.
- **Phase 2:** Bun fork, material CDN, android-lab, site reproducibility, OpenPets/@aphrody/pets, assets-lab (private), modern-web plugin.
- **Phase 3:** wgpu fork and `bun:wgpu`, gpu-bench, raster (Skia vs tiny-skia), gpu-unlock and compute tuning.

## Known hard problems, stated

- **The aphrody merge is large.** About 343 crates and a Zed history of 41k commits bring dependency-version conflicts with fw. The PC's aphrody checkout also has uncommitted edits from other sessions, and they must land first.
- **AOSP needs 64 GB of RAM, more than fr1 has.** A full Android build needs a private Cloud Build pool or a bigger host; the release train ships apps, not AOSP.
- **moon's remote cache against NativeLink is documented for bazel-remote.** It must be proven on our setup in Phase 1, and Bazel is the fallback.
- **gcloud credits are unknown until the billing console is read.** Spend stays behind the budget cap.
