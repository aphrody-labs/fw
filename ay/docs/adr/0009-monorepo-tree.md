# ADR-0009: Monorepo tree and build orchestration

Status: accepted, 2026-11-17 (owner request: decide the best tree and architecture). Supersedes the tree part of ADR-0001
and the "native workspaces" part of ADR-0002; the Nix, LLVM and language rules of ADR-0002 and ADR-0008 stand.

## Context
Live study, 2026-11-17, through `gh api repos/<owner>/<repo>/contents` and the root manifests:

| Repository | First-party code | Vendored or upstream code | Entry point | Cargo workspace |
|---|---|---|---|---|
| rust-lang/rust | `compiler/` (80 crates), `library/`, `src/` | submodules (`src/llvm-project`, `src/gcc`, `src/tools/cargo`, ...) | `x.py` over `src/bootstrap` | resolver 2, about 45 members |
| oven-sh/bun | `src/` (119 dirs, one per crate), `packages/` | recipes `scripts/build/deps/*.ts` fetch into an excluded `vendor/` | `scripts/build.ts` generating ninja | resolver 2, about 137 members, `exclude = ["vendor"]` |
| servo/servo | `components/` (40) | `third_party/` (1 dir) | `mach` (Python, uv) | resolver 2, 311 workspace dependencies |
| denoland/deno | `cli/`, `ext/` (31), `libs/` (26), `runtime/` | submodules for test data only | `x` and cargo | resolver 2, 77 members |
| chromium/chromium | about 60 top-level component dirs | `third_party/` (296 dirs), roster in `DEPS` | gn + ninja, gclient | none (Rust through gn) |
| llvm/llvm-project | about 28 flat subproject dirs, each with its own `test/` | `third-party/` | CMake + ninja, lit | none |
| zed-industries/zed | `crates/` (253, flat) | none in tree | cargo, `script/`, `tooling/xtask` | resolver 2, 261 members, `default-members` |
| bevyengine/bevy | `crates/` (67), `benches/`, `examples/`, `tests/` | none | cargo, `tools/` | resolver 3, `[workspace.lints]` |
| rust-lang/rust-analyzer | `crates/`, `lib/` | none | `cargo xtask` | resolver 2, about 45 members |
| vercel/next.js | `packages/`, `crates/`, `turbopack/` | none | Turborepo over pnpm | resolver 2 |

Our own measurements (hyperfine 2.0.0, 5 runs, `cargo metadata --no-deps --locked --offline`, i7-13700F, Windows 11,
cargo 1.102.0-nightly): the `aphrody` checkout (303 members) loads in 490.5 +- 57.4 ms, the `bun` fork (137 members) in
144.8 +- 3.4 ms. A flat `crates/*` workspace of hundreds of members costs half a second of manifest work, not minutes.

Observed regularities: (1) first-party code and vendored code are separated at the root and the vendored tree is excluded
from the workspace; (2) Rust-heavy repositories (zed, bun, deno, servo) use one root manifest with a
`[workspace.dependencies]` table of 150 to 514 entries and lint inheritance; (3) the entry point is a thin script in the
repository's own language, never a general build system: `x.py`, `mach`, `x`, `scripts/build.ts`, `cargo xtask`; (4) no
repository of the set builds with Bazel, Buck2, Nx or moon (llvm only carries a Bazel overlay under `utils/bazel`).

Orchestrator facts, same day (`gh api repos/<r>/releases/latest`): Bazel 9.3.2 (2026-11-05); Nx 24.0.6 (2026-11-10);
Turborepo 2.11.11 (2026-11-11); moon 2.6.5 (2026-10-29); just 1.59.0 (2026-10-15); ninja 1.13.2 (2025-11-20); Buck2 ships
dated rolling tags (`2026-11-15`) and its own documentation says the open-source remote execution "may be less polished".
No first-party benchmark of any of them exists on the public record that compares them on one workload (BuildBuddy's review
of Buck2 and the moon comparison page are the closest, and both state their limits); we therefore decide on cost of
adoption and measured workspace overhead, not on published speed claims.

## Decision
One tree, one orchestration rule, for `ay` and as a template any Aphrody repository adopts alone (independence rule of
`docs/architecture.md`).

```
<repo>/
  <identity>.toml    the one identity file (ay.toml, naming.toml); names, hosts, targets are read from it
  Cargo.toml         root workspace: resolver "3", members ["crates/*"], [workspace.package] edition 2024,
                     [workspace.dependencies] (every version once), [workspace.lints], exclude ["third_party", "vendor"]
  rust-toolchain.toml  channel pin (stable by default; nightly only for a stated feature)
  package.json       Bun workspace: workspaces ["packages/*", "apps/*"], script "check" = the repository gate
  crates/            Rust crates, flat, one folder each, tests in the crate (src unit tests, tests/ integration)
  packages/          Bun/TypeScript libraries, flat, @aphrody/<name>, tests as *.test.ts beside the code
  apps/              programs a person runs, one folder each
  <native>/          one root per non-Cargo language when its first unit lands (corelib/ for C, assembly, WAT; dotnet/;
                     python/ with pyproject), each with its own conventional manifest
  third_party/       upstreams as submodules, relative URLs, one checkout each; never in a workspace
  vendor/            `cargo vendor` and fetched dependency caches only; gitignored
  tests/             cross-unit integration and end-to-end suites (needs two or more units)
  benches/           cross-unit benchmarks (criterion or mitata); per-crate benches live in `crates/<c>/benches`
  tools/             typed Bun scripts: every repeatable flow; `cargo xtask` only when a task must link workspace crates
  docs/              adr/, spec, science/, research/, guides/
  .github/workflows/ thin: each job runs one repository script (`bun run check`, cargo gates), never inline logic
```

Rules:
1. **Orchestration is native**: `cargo` for Rust, `bun` workspaces for TypeScript, the language's own tool for the rest,
   and typed Bun scripts in `tools/` as the single cross-language entry (`bun run check`, `bun tools/<flow>.ts`). ninja
   or gn run only inside an upstream that already uses them (Chromium through gclient, the Bun fork through
   `scripts/build.ts`); we never write ninja or gn files for our own units.
2. **Escalation criterion** (measured, not assumed): adopt a remote-cache build system only when a clean CI job exceeds
   15 minutes after `cargo` and `bun` caches, or when two languages share an incremental graph that the native tools
   cannot express; the first candidate then is Buck2 or Bazel with remote execution, evaluated by a benchmark on our
   workload in `benches/` first.
3. **One version table**: a dependency version is written once, in `[workspace.dependencies]` or the Bun workspace
   catalog; members inherit (`dep.workspace = true`, `[lints] workspace = true`).
4. **Flat, named by function**: no layer directories (`system/`, `frameworks/`), no nesting deeper than
   `<root>/<unit>/`; a unit moves by moving a folder.
5. **Tests where the code is**, cross-unit tests in `tests/`, benchmarks in `benches/`; a folder is created only with a
   working unit (`AGENTS.md`).
6. **Vendored tree is data**: `third_party/` holds submodules we track; patches live in a fork repository, not in a
   patch directory of ours.

Current state: `ay` already matches (resolver 3, `crates/*`, `packages/*`, `apps/*`, `corelib/`, `tools/`, `docs/`,
`third_party/`); `docs/science/` is added by this record. `aphrody` (owned by another agent) uses `vendor/` for forked
upstreams and `tooling/` for xtask like Zed; it converges to `third_party/` when its owner next touches it. `bun` keeps
upstream's layout (a fork never reshapes its upstream, so merges stay mechanical).

## Rejected, one line each
- **Bazel**: no large Rust or Bun repository of the study uses it, Starlark rules for Rust and TypeScript are a second build
  description to maintain, and our measured workspace overhead (0.14 to 0.49 s) leaves nothing for it to win.
- **Buck2**: rolling tags and an openly "less polished" remote execution; the best fit later (rule 2), not today.
- **moon**: the task runner duplicates what `cargo` and `bun run` already do; its own comparison page warns that it may
  be inaccurate, and only toolchain management is unique.
- **Nx and Turborepo**: JavaScript-first; Rust support is by plugin or script wrappers, and the remote cache is a
  vendor service (Nx Cloud, Vercel); next.js, the one adopter in the set, still needs a separate Cargo workspace.
- **just or make as entry point**: a second language for the same job; Bun scripts are typed, testable and run on every OS.
- **Nested workspaces or one repository per crate**: breaks the single `[workspace.dependencies]` table and atomic
  cross-crate changes (zed: 253 crates in one workspace; bun: 137).
- **Copying an upstream's vendor tree into ours**: the old monorepo duplicated about 150 Bun crates beside the fork they
  came from (ADR-0001); submodules and fetch recipes (as `bun` does for its 19 dependencies) keep one checkout each.

## Consequences
- A new repository starts from the tree above and the root manifests of `ay`; no repository requires a sibling.
- `bun run check` stays the gate of every repository; a repository with Rust adds `cargo fmt --check`, `clippy`, `test`.
- Evidence behind this record is dated 2026-11-17 and reproducible with the `gh api` calls named above.
