# Layout 2026: fw

The target layout of this repository, the rules behind it, and the decision for every tracked path. The map is
`layout.toml` (one file per repository, read by `bun tools/layout.ts`); the tables at the end of this page are generated from it and from
`git ls-files`, so a path that is not in the map fails `bun tools/layout.ts check` in CI.

## What the map owns

| ecosystem | declared in layout.toml | written by `layout.ts sync` | checked by `layout.ts check` |
| --- | --- | --- | --- |
| Cargo | `[cargo] members`, `[cargo.crate.NAME]` (home: repo + path, git + rev or tag, registry version; `package` for a rename; options), `[[cargo.patch]]`, `[cargo.package]` | `[workspace] members`, `[workspace.dependencies]`, `[patch.*]`, chosen `[workspace.package]` keys | members use `workspace = true`, no path leaves the repo, direction fw <- aphrody <- ay, pinned git, home exists and has the declared package name, aphrody: no submodules, no vendor/ |
| Bun | `[bun] workspaces`, `exclude` | root `package.json` `workspaces` | every package.json is a member or excluded with a reason, workspace packages use `workspace:*` |
| uv | `[uv] members`, `exclude` | `[tool.uv.workspace]` | every pyproject.toml is a member or excluded |
| Gradle | `[gradle] root, include, catalog` | `include(...)` in `settings.gradle.kts` | the version catalog exists |
| .NET | `[dotnet] root, solution, projects` | the `.slnx` | `Directory.Build.props` and `Directory.Packages.props` exist, central package management is on, no `Version=` on a PackageReference |
| CMake | `[cmake] projects` | nothing | every project has a `CMakePresets.json` |
| all | `[[area]]`, `[[moved]]` | `docs/architecture/layout-2026.md` (`layout.ts doc`) | every tracked path is covered by an area, no reference to a moved path remains |

A move, a rename, a repository switch or a patch edits one entry of that table and nothing else. Proof on a scratch clone (host fr1): a crate moved from fw to ay, a
crate renamed, and three crates patched with a fork, each followed by `cargo check` of the dependents, are recorded in the commit that introduced the tool.
The stable name of a crate is the dependency key (`fw-secrets = { workspace = true }`); the code imports the library name, which is the `[lib] name` of the
crate or the key with dashes replaced by underscores. A renamed package keeps its old key in the map (`package = "fw-vault"`) and its `[lib] name`.

## Conventions read today

Sources fetched on 2026-11-21; each rule below cites one.

- Cargo (https://doc.rust-lang.org/cargo/reference/workspaces.html): one `Cargo.lock` and one `target/` per workspace; `[patch]`, `[replace]` and `[profile]` are read from the root only; `members` accepts globs and `exclude` removes paths; a virtual workspace needs an explicit `resolver`; `[workspace.package]` and `[workspace.dependencies]` are inherited with `.workspace = true`; the recommended flat layout is `members = ["crates/*"]`. Hence: crates stay in `crates/*`, every cross-crate dependency goes through `workspace = true`.
- Bun (https://bun.com/docs/pm/workspaces): `workspaces` in the root `package.json` lists globs (`packages/*`), negative patterns exclude, the `workspace:` protocol links local packages, `catalog:` shares versions, the root holds config and `packages/` holds the code. Hence: Bun packages live in `apps/*` and `packages/*` only, and the repository root has no source (the root TypeScript in `src/` and `tests/` moves into `packages/fw`).
- uv (https://docs.astral.sh/uv/concepts/projects/workspaces/): `[tool.uv.workspace] members` globs, `[tool.uv.sources] x = { workspace = true }`, one `uv.lock`, one `requires-python` for all members. Hence: Python projects live under `python/`, matching the member glob.
- CMake (https://cmake.org/cmake/help/latest/manual/cmake-presets.7.html): `CMakePresets.json` (committed) and `CMakeUserPresets.json` (local, ignored) at the project root, with configure, build, test and workflow presets, `inherits` and `include`. Hence: each CMake project has a presets file and its own root under `cpp/`.
- .NET (https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-by-directory, https://learn.microsoft.com/en-us/nuget/consume-packages/central-package-management, https://devblogs.microsoft.com/dotnet/introducing-slnx-support-dotnet-cli/): `Directory.Build.props` is imported by every project below it, `Directory.Packages.props` with `ManagePackageVersionsCentrally` holds `PackageVersion` entries and projects carry `PackageReference` without `Version`, `.slnx` is the XML solution format supported by the dotnet CLI from SDK 9.0.200. Hence: one `dotnet/` root with the two props files and a generated `.slnx`.
- Gradle (https://docs.gradle.org/current/userguide/organizing_gradle_projects.html, https://docs.gradle.org/current/userguide/version_catalogs.html): one root with `settings.gradle.kts` and `include()`, the catalog at `gradle/libs.versions.toml` read as `libs`, shared build logic in an included build or `buildSrc`. Hence: one `kotlin/` root per repository with the catalog at `kotlin/gradle/libs.versions.toml`.
- Reference repositories (root listings read with `gh api repos/<owner>/<repo>/contents` today): astral-sh/uv keeps Rust in `crates/` and Python in `python/` with `scripts/` and `docs/`; zed-industries/zed (the parent of aphrody) keeps `crates/`, `extensions/`, `assets/`, `script/` and `tooling/`; bevyengine/bevy keeps `crates/`, `examples/`, `benches/` and `tools/`; android/nowinandroid keeps `build-logic/`, `gradle/libs.versions.toml` and one directory per module; oven-sh/bun keeps `packages/`, `src/`, `scripts/` and `test/`. fw follows the same shape: one top-level directory per ecosystem, flat members inside, tooling in `tools/`.

## Rules for this repository

1. One top-level directory per ecosystem root: `crates/` (Cargo), `apps/` and `packages/` (Bun), `python/` (uv), `kotlin/` (Gradle), `dotnet/` (MSBuild), `cpp/` (CMake). A project of an ecosystem outside its root is a defect, except nested language loaders that carry their own root (listed in the map with the reason).
2. Declarations, not edits: membership, homes, patches and versions of shared dependencies are written from `layout.toml`; hand edits of the generated blocks are overwritten and fail CI.
3. Dependency direction fw <- aphrody <- ay; no lower repository names a higher one. aphrody pins git dependencies and has no submodules and no `vendor/`.
4. Targets: Windows 11, recent Linux and wasm32. macOS is out of scope; no macOS-only work is added.
5. Prefer the upstream leader over in-house code: the decision per module is recorded below and executed in order.
6. Pure Rust is the direction (owner decision, audit in `docs/research/pure-rust-audit-2026-11-21.md`): a TypeScript, JavaScript, Python or Java source file that is not in `docs/research/pure-rust-baseline.txt` and not under a deliberate `[rustonly] allow` entry fails `bun tools/layout.ts check`. The baseline only shrinks as ports land; a port goes to its final Rust home (fw subcommand, workers-rs Worker, crate), so the move table never relocates code that is about to be replaced.

## Proof of relocation safety

Scratch clones of fw, aphrody and ay on fr1 (`/srv/xbuild/scratch/layout-proof`, shared target), `cargo check` of the dependents after editing the map only (plus the directory move itself):

| change | map edit | dependent checked | result |
| --- | --- | --- | --- |
| move `fw-depot` from fw to ay | fw: `members` entry removed; ay: `[cargo.crate.fw-depot]` loses `repo` and `path`; ay gains `[cargo.crate.ureq]` and `[cargo.package]` keywords and categories (registry homes the moved crate inherits) | `cargo check -p ay-build --features deps,siso` | baseline Finished in 10.97s, after Finished in 3.13s |
| rename `fw-secrets` to `fw-vault` (package and directory; key and `[lib] name` kept) | fw: members path, `[cargo.crate.fw-secrets] package = "fw-vault"`; ay: `path` and `package` | `cargo check -p fw-model` (fw), `cargo check -p ay-accounts --features secrets` (ay) | Finished in 52.28s and in 37.79s; importers still say `fw_secrets::` |
| patch `kube`, `kube-client`, `kube-core` with the aphrody-labs/kube fork at rev c1decf9c415561f877b51325ada024b0b8139eac | three `[[cargo.patch]]` entries | `cargo check -p fw-cluster --features kube` | Finished in 0.82s; `cargo tree` resolves kube v5.1.0 from `https://github.com/aphrody-labs/kube?rev=c1decf9c` |

Equivalence of the adoption itself: `cargo metadata` (every dependency with kind, requirement or path, features, default-features, source) before and after `layout.ts sync` and `fix` is identical in all three repositories (fw 1,678 lines, aphrody 9,008, ay 896).
A renamed package whose importers use the library name needs the `[lib] name` kept; an alias key changes the extern crate name to the key, which is why the rename above keeps `[lib] name = "fw_secrets"`.

## One repository: aphrody and ay inside fw

fw is the monorepo and the stack root. aphrody and ay were imported on 2026-11-21 with their full history (`bun tools/monorepo/import.ts`: `git filter-repo --to-subdirectory-filter`, then a merge of the unrelated histories; fw 390, aphrody 41,072 and ay 256 commits, 41,721 after the merge).

Decisions and reasons:

1. Prefixes `aphrody/` and `ay/`, not flattened into the ecosystem roots yet. `git merge -X subtree=aphrody zed-industries/zed` needs the whole Zed tree under one prefix, so aphrody keeps its upstream shape (crates, assets, extensions, script, tooling). ay shares directory names with fw (apps, packages, tools, docs, config, kotlin, python); folding them into the roots is a sequence of measured `layout.ts move` steps after the workspaces are one, not part of the import.
2. One workspace per ecosystem at the root, members from all three origins: Cargo `crates/*`, `aphrody/crates/*`, `ay/crates/*` (tools/monorepo/unify.ts, decisions in unify.toml); Bun `apps/*`, `packages/*` and the ay packages and apps; uv `python/*` and `ay/python/*`; one Gradle build in `kotlin/` including `ay/kotlin/aphrody-theme`. Until each step lands, the imported tree is listed under `exclude` of that ecosystem in layout.toml, with its own workspace and lockfile untouched.
3. One CI, one release workflow driven by `release.toml` and `tools/release-plan.ts` (from ay), one place for secrets. Workflows of the imported trees under `aphrody/.github` and `ay/.github` do not run (GitHub reads only the root `.github`); their jobs move to the root workflows in gated batches.
4. Zed keeps following upstream through the prefix: the hourly sync merges `upstream/main` with `-X subtree=aphrody` and the deterministic conflict resolver of `aphrody/tooling/bun/upstream-sync.ts`, ported to the prefix.

## Decision for every tracked path

<!-- layout:begin generated (bun tools/layout.ts doc, from layout.toml and git ls-files of 16610 paths) -->

### Every tracked path, by area

| area | files | decision | reason |
| --- | ---: | --- | --- |
| `.agents` | 3 | keep | agent workflows and rules live in .agents at the repository root; AGENTS.md and README.md point to the onboarding journey there |
| `.cargo` | 1 | keep | Cargo reads .cargo/config.toml from the workspace root |
| `.claude-plugin` | 1 | keep | marketplace.json at the root is how `claude plugin marketplace add` finds the aphrody plugin |
| `.editorconfig` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `.fdignore` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `.gitattributes` | 1 | keep | Git reads these from the repository root |
| `.github` | 23 | keep | GitHub reads workflows, actions, CODEOWNERS and dependabot from .github |
| `.gitignore` | 1 | keep | Git reads these from the repository root |
| `.gitmessage` | 1 | keep | Git reads these from the repository root |
| `.gitmodules` | 1 | keep | Git reads these from the repository root |
| `.markdownlint-cli2.jsonc` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `.python-version` | 1 | keep | uv reads the Python project, workspace and pin from the root |
| `.rgignore` | 1 | keep | editors and search/lint tools discover their config upward from the root |
| `AGENTS.md` | 1 | keep | agent rules and the style contract; agents load them from the root |
| `aphrody` | 9754 | keep | the Zed fork, imported with its full history under one prefix so `git merge -X subtree=aphrody zed-industries/zed` keeps following upstream; its internal layout is upstream Zed's (crates, assets, extensions, script, tooling) |
| `apps` | 46 | keep | Bun workspaces convention: deployable apps live in apps/* |
| `apps/chromium-dash` | 9 | keep | Bun workspace members belong in apps/*, not nested in a Rust crate |
| `ay` | 1888 | keep | the SDK, cloud and edge repository, imported with its full history; its crates, packages, apps, kotlin and python roots fold into the workspaces of this repository step by step (layout.toml moves) |
| `bun.lock` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `bunfig.toml` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `Cargo.lock` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `Cargo.toml` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `cloudflare` | 30 | keep | wrangler configs sit beside the worker sources they deploy; deploy workflows and fw-edge refer to them |
| `cluster` | 5 | keep | host topology, build and backup definitions read by tools/fr1.ts and the fleet scripts; roles, not addresses |
| `compose.yaml` | 1 | keep | Docker tooling reads these from the build context root |
| `configs` | 65 | keep | agent and tool configuration sources (claude scripts, heretic), consumed by the plugin sync |
| `core` | 19 | keep | single-file native cores, one directory per language (asm, c, cuda, go, java, wasm, webgpu, py) |
| `cpp` | 4 | keep | CMake projects get their own top-level root with CMakePresets.json (ay already uses cpp/) |
| `crates` | 1168 | keep | Cargo convention: flat crates/* members, one directory per crate |
| `deny.toml` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `Dockerfile` | 1 | keep | Docker tooling reads these from the build context root |
| `docs` | 280 | keep | documentation tree; docs/crates is generated catalog data, docs/architecture holds the layout contract |
| `dotnet` | 6 | keep | .NET projects get one root with Directory.Build.props, Directory.Packages.props (central package management) and a .slnx |
| `flake.nix` | 1 | keep | Nix flakes and legacy entry points are looked up at the root |
| `fw.soul.json` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `fw.targets.json` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `justfile` | 1 | keep | just discovers the justfile at the root |
| `kernel.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `kotlin` | 22 | keep | a Gradle build has one root with settings.gradle.kts, a version catalog and the wrapper; ay uses kotlin/ at the top level |
| `layout.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `LICENSE` | 1 | keep | repository front matter and legal files, read by GitHub and package registries at the root |
| `LIFETIMES_AND_STYLE.md` | 1 | keep | agent rules and the style contract; agents load them from the root |
| `MAP.tsv` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `native` | 13 | keep | freestanding kernel-mode crates outside the workspace (own targets); declared in Cargo exclude |
| `nix` | 5 | keep | NixOS modules and host definitions referenced by flake.nix |
| `os` | 28 | keep | operating-system crates and images outside the hosted workspace (own targets) |
| `org.plan.json` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `org.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `package.json` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `packages` | 546 | keep | Bun workspaces convention: libraries in packages/* |
| `packages/chromium` | 7 | keep | Bun workspace members belong in packages/*, not nested in a Rust crate |
| `packages/chromium-releases` | 40 | keep | Bun workspace members belong in packages/*, not nested in a Rust crate |
| `plugins` | 2493 | keep | the Claude plugin source tree; its layout (skills/, agents/, hooks/) is fixed by the plugin format |
| `product.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `pyproject.toml` | 1 | keep | uv reads the Python project, workspace and pin from the root |
| `python` | 12 | keep | uv workspace members live in python/*, not under a Bun package |
| `README.md` | 1 | keep | repository front matter and legal files, read by GitHub and package registries at the root |
| `release.toml` | 1 | keep | declarative root file: the release matrix of every product and the release train (tools/release-plan.ts, tools/release-publish.ts, docs/release/model.md); moved from ay |
| `REVIEW.md` | 1 | keep | repository front matter and legal files, read by GitHub and package registries at the root |
| `rust-toolchain.toml` | 1 | keep | Cargo and the Rust toolchain read these from the workspace root |
| `src` | 20 | move -> `packages/fw/src` (blocked) | Bun convention: the root holds workspace config only, code lives in a package Blocked: package.json main, bin and scripts, plugin runtime paths and tests import src/*; moves with its own gate (bun test over tests/) |
| `tests` | 16 | move -> `packages/fw/test` (blocked) | tests move with the package they test Blocked: moves together with src |
| `third_party` | 5 | keep | gitlinks to the kernel sources the kernel lot builds (update = none, shallow) |
| `THIRD_PARTY_NOTICES.md` | 1 | keep | repository front matter and legal files, read by GitHub and package registries at the root |
| `tools` | 60 | keep | repository scripts (Bun) with their own tests; one script per task |
| `tsconfig.json` | 1 | keep | Bun workspace root: install, lockfile, runtime config and TypeScript base config are read from the root |
| `upstreams.toml` | 1 | keep | declarative product, naming, release and layout configuration: the single root source other files are derived from |
| `uv.lock` | 1 | keep | uv reads the Python project, workspace and pin from the root |
| `vercel` | 2 | keep | vercel.json and api/ are read from this directory by the Vercel project root setting |
| `xtask` | 2 | keep | cargo xtask convention: a workspace member named xtask at the root |

### Move table (old path -> new path)

| old path | new path | reason | status |
| --- | --- | --- | --- |
| `core/cpp` | `cpp` | CMake projects get their own top-level root with CMakePresets.json (ay already uses cpp/) | done |
| `crates/fw-browser-chromium/apps/chromium-dash` | `apps/chromium-dash` | Bun workspace members belong in apps/*, not nested in a Rust crate | done |
| `crates/fw-browser-chromium/packages/chromium` | `packages/chromium` | Bun workspace members belong in packages/*, not nested in a Rust crate | done |
| `crates/fw-browser-chromium/packages/chromium-releases` | `packages/chromium-releases` | Bun workspace members belong in packages/*, not nested in a Rust crate | done |
| `packages/ai-os/python` | `python` | uv workspace members live in python/*, not under a Bun package | done |
| `packages/ai-os/native` | `dotnet` | .NET projects get one root with Directory.Build.props, Directory.Packages.props (central package management) and a .slnx | done |
| `core/kotlin` | `kotlin` | a Gradle build has one root with settings.gradle.kts, a version catalog and the wrapper; ay uses kotlin/ at the top level | done |
| `src` | `packages/fw/src` | Bun convention: the root holds workspace config only, code lives in a package | blocked: package.json main, bin and scripts, plugin runtime paths and tests import src/*; moves with its own gate (bun test over tests/) |
| `tests` | `packages/fw/test` | tests move with the package they test | blocked: moves together with src |

<!-- layout:end generated -->

## Upstream or in-house: decision per module

Data pulled on 2026-11-21 from the crates.io API (`https://crates.io/api/v1/crates/<name>`: `max_version`, `recent_downloads` over 90 days, release date, license), the GitHub API
(`gh api repos/<owner>/<repo>`: `pushed_at`, archived, stars), the npm registry and downloads API (last week) and the PyPI JSON API. "rec" is crates.io `recent_downloads`.
KEEP means the in-house code does something the upstream does not, or no maintained upstream exists. License is recorded only; any license question is queued, work goes on.
Targets are Windows 11, Linux and wasm32; macOS candidates are ignored.

| area | in-house path | function | decision | upstream (version, rec, release, license) | reason |
| --- | --- | --- | --- | --- | --- |
| protocol | `crates/fw-a2a` | A2A v1 types, protobuf, client, server, gRPC | REPLACE (phase 2) | a2a-lf 0.4.2 (rec 198,691, 2026-10-16, Apache-2.0), a2a-client-lf 0.2.9, a2a-server-lf 0.5.3, a2a-pb 0.3.2, a2a-grpc 0.3.11 (a2aproject/a2a-rs, pushed 2026-11-20) | official SDK covers JSON-RPC, REST, gRPC, SSE; keep `coordinator/` as the adapter; wasm32 client unverified, wire parity needs retesting |
| protocol | `fw-proto/src/mcp.rs`, `fw-toolcall/src/mcp/*`, `fw-gemini/src/runtime/mcp_client.rs` | MCP server and client | KEEP until gated | rmcp 3.11.0 (rec 31,339,790, 2026-11-17, Apache-2.0) | ours passes the official conformance suite on three spec dates; switch only after rmcp passes `tools/mcp-conformance` |
| protocol | `fw-fx/src/acp.rs` | ACP JSON-RPC server | REPLACE (gated) | agent-client-protocol 5.4.0 (rec 2,659,387, 2026-11-18, Apache-2.0) | official crate; needs fx-parity tests first |
| protocol | `fw-proto/src/server.rs` | hand-rolled HTTP/1.1 over TcpStream | REPLACE (phase 2) | axum 0.8.9 (rec 174,452,863, 2026-04-14, MIT) via fw-http | ours answers 411 to chunked bodies and closes every connection; MCP conformance gates the change |
| protocol | `crates/fw-http` | HTTP client and axum server | KEEP | reqwest 0.13.5, axum 0.8.9, h3 0.0.8 | already a thin wrapper over upstream |
| agent | `fw-agent`, `fw-toolcall`, `fw-gemini`, `fw-fx`, `fw-hermes-claw`, `fw-apm`, `fw-agent-context`, `fw-ring3-agent`, `fw-unified-stack` | Codex-compatible agent, tool schema subset, Gemini CLI driver, ports of fx, Hermes, apm | KEEP | rig-core 0.47.0 (rec 2,295,823), genai 0.7.5, adk-rust 2.3.0, libaipm 0.25.1 (rec 270) | upstreams are LLM-app libraries without apply_patch, command guard or rollout store; no maintained Rust upstream for the ports |
| inference | `fw-llama`, `fw-llama-sys` | llama.cpp bindings, runtime dlopen, mtmd, llguidance | KEEP | llama-cpp-2 0.1.161 (rec 946,610, 2026-11-03) | upstream links at build time; we load at runtime so one binary runs without the DLL; revisit if upstream gains libloading |
| inference | `fw-model`, `fw-ml`, `fw-ggml` | candle inference, HF client, GGUF reader and ggml-rpc | KEEP (GGUF reader may move to candle-core) | mistralrs 0.8.1, fastembed 7.7.1, hf-hub 1.0.0, candle-core 0.11.1 | already built on candle, hf-hub; ggml-rpc wire has no upstream |
| rag | `fw-rag-core`, `fw-rag`, `fw-browser-index`, `fw-corpus`, `fw-crates` | chunkers, hybrid search, HNSW int8 + mmap, crawler into FTS5, crates dataset | KEEP | text-splitter 0.33.3, tantivy 0.27.0, usearch 2.26.6 (rec 827,263), crates-index 4.0.0, rustsec 0.34.1 | byte-parity goldens with the TS side; usearch is C++ and not benchmarked against our int8 HNSW yet (bench first); advisory parsing may move to rustsec |
| security | `fw-secrets`, `fw-google` | secret store, Workspace client | KEEP | secrecy 0.10.3 (last release 2024-10-09), aes-gcm 0.11.1, argon2 0.6.0, yup-oauth2 12.1.2 | already composes the upstream primitives |
| process | `fw-proc/src/{group,unix,windows,no_window}.rs` | process group kill-on-drop, Windows Job Object, no-window flag | REPLACE (mechanical) | process-wrap 10.0.1 (rec 17,077,292, 2026-09-23, Apache-2.0 OR MIT; features job-object, process-group, creation-flags, kill-on-drop, tokio1) | direct overlap; ay-re already depends on it; keep `elevate.rs`, `lease.rs`, `lock.rs`, `port.rs` |
| userland | `fw-uutils` | 40 string-in string-out applets, wasm playground | REPLACE natively (phase 2) | uutils coreutils 0.13.0 (rec 31,437, 2026-11-20, MIT; 24,407 stars), uu_* crates, findutils 0.11.0, diffutils 0.6.0, sed 0.3.0 | about 109 upstream utilities; Windows supported; wasm target is wasm32-wasip1; keep ours for the browser playground until measured |
| files | `fw-files` | Yazi-style file engine | KEEP | yazi-fs 26.5.6 (rec 3,774), alphanumeric-sort 1.5.8 | Yazi crates are tied to its Lua runtime; natural sort may move to alphanumeric-sort |
| cluster | `fw-cluster` | consistent hash, KV-prefix routing, llm-d scoring | KEEP | hashring 0.3.7, kube 5.1.0 (used) | prefix affinity and scheduler have no upstream |
| compute | `fw-compute` | hardware probe with hardcoded DLL and /dev paths | REPLACE (probe) | wgpu 30.0.2 (rec 20,785,149, 2026-11-12), cudarc 0.19.10, sysinfo 0.40.4 | hardcoded paths break the no-hardcoding rule; adapter enumerates with wgpu and cudarc; watch fw-bin size |
| ui | `fw-ui-blitz` | hand-written CRC32 and PNG writer | REPLACE (mechanical) | png 0.18.1 (rec 123,653,601, 2026-02-14, MIT OR Apache-2.0) | delete crc32, write_chunk, encode_rgba_png behind a 6-line wrapper |
| ui | `fw-m3`, `fw-material`, `fw-wayland-wgpu`, `fw-ostd-wasm` | M3 tokens, Symbols constants, Wayland on WebGPU and wasm32 | KEEP | material-colors 0.5.0, @material-symbols/svg-400 0.49.0 (npm), wayland-server 0.31.14, smithay 0.7.0 | no current Rust crate carries Material Symbols; upstream Wayland needs Unix sockets and has no wasm32 path |
| nix | `fw-nix` | nixbase32, NAR encoder for one file | REPLACE partly (mechanical) | nix-archive 0.6.0 (rec 3,303, 2026-09-17, Apache-2.0), nix-base32 0.2.0 | new crate: pin exactly and diff against `nix-store --dump`; keep fixed-output store path code |
| platform | `fw-linux`, `fw-depot`, `fw-edge`, `fw-drive` | kernel pins, depot_tools, Worker, Drive on D1 and R2 | KEEP | worker 0.9.0, opendal 0.59.5, teapot_tools 0.4.3 (rec 11) | no maintained upstream for depot_tools; edge already uses `worker` |
| browser | `fw-browser-*` | CEF, CDP pool and guardrails, exit routing, detection | KEEP; bump cef | cef 156.7.0+156.0.31 (pin =156.6.0), chromiumoxide 0.9.1, boringtun 0.7.1 | guardrail and pairing layer over several engines; cef is one release behind |
| text | `fw-translate-core` | comment extraction, script detection | KEEP | whatlang 0.18.0, lingua 1.8.0 | only Japanese script detection is needed |

Executed first (mechanical, no behavior change), in this order: `fw-proc` process groups to process-wrap, `fw-ui-blitz` PNG encoder to png, `fw-nix` NAR and nixbase32 to nix-archive and nix-base32. Phase 2 (gated by conformance or parity tests): a2a-lf set, the axum server in fw-proto, fw-uutils to uutils, the ACP crate, the compute probe.
Version drift found on the way: `fw-browser-chromium` pins cef =156.6.0 (latest 156.7.0+156.0.31).

License queue (recorded, no decision taken): slint 1.19.0 (GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0), nix-compat 0.0.0-pre (GPL-3.0-only), nix-nar 0.6.0 (Apache-2.0 OR LGPL-2.1-or-later), msal 0.8.19 (LGPL-3.0-or-later), ffmpeg-next 9.1.0 (WTFPL), n8n-workflow 2.16.0, uhlc 0.9.0 (EPL-2.0 OR Apache-2.0), MPL-2.0 crates (webauthn-rs, symphonia, ostd, uniffi, cbindgen).

Decisions for aphrody and ay modules (same data, same date): ay `ay-media` HLS parser REPLACE by m3u8-rs 6.0.1 (rec 381,412, MIT; ours ignores master playlists, keys and byte ranges); ay `auto-sdk/src/auth/{jwt,totp,passkey,password}.rs` REPLACE (partial) by jsonwebtoken 12.0.0, totp-rs 6.0.0, webauthn-rs 0.6.2 (wasm unverified: check the auto-ffi build first); ay `ay-accounts` PKCE and device-code flows REPLACE (partial) by openidconnect 4.1.0; aphrody `aphrody_google/src/auth.rs` and `ai.rs` REPLACE (partial) by google-cloud-auth 1.19.0 and google-cloud-aiplatform-v1 1.19.0; aphrody `aphrody_pilot` backends REPLACE (phase 2) by uiautomation 0.25.3 and atspi 0.30.0; KEEP the rest (`aphrody_av`, `nsh-*`, `aphrody_ui` layers, `ay-re`, `ay-vault`, `ay-balancer`, `env-kit`, `automate`, npm packages). Version drift: aphrody pins ed25519-dalek 2.1 (latest 3.0.0) and zbus 5.13 (latest 6.5.0).
