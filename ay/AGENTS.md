# AGENTS.md

Contract for every agent working in this repository. Names come from `ay.toml`.

## Scope
- This repository is the SDK of Aphrody: C ABI, OpenAPI, docs, spec, standards, corelib, libc and the
  design system, in whatever language fits each unit (C and its derivatives, assembly, Rust, Zig,
  WebGPU, Python, .NET, Bun). LLVM first, tree-sitter, C-ABI FFI, LTO, AOT, unified lib/MSVC, cross
  arch/OS/GPU/platform (ADR-0008). The complete OS is `aphrody-dev/aphrody`; this SDK is independent of it
  and versioned on its own. Satellites (`ay.toml` [repos], [registries]) build on both.
- The products and their definition of done are in `docs/spec.md`; the tree and toolchain in
  `docs/architecture.md` and `docs/adr/`; the full scope, row by row, in `docs/features.toml`.
- A feature is done only with a proof in the matrix (test id, screenshot path or artifact URL);
  `bun run check` refuses anything else.

## Rules
- No prototypes, scaffolds or placeholders: add a folder only with a working, tested unit.
- Follow the tree in `docs/architecture.md`: programs in `apps/`, Bun packages in
  `packages/`, Rust in `crates/`, C and assembly in `corelib/`, upstreams as submodules in `third_party/`.
- Assemble upstream first; never copy an upstream's code or directory layout into the tree.
- Nothing hardcoded: names, hosts and domains come from `ay.toml`; versions are read live.
- Run the repository's own checks before every push (`bun run check`, plus `cargo` checks for `crates/`).
- Heavy builds run on the `build` host, tests on `test`, production only on `prod` (`ay.toml` [hosts]).
- Work on `main`; no branches, worktrees, stash or reset. No AI attribution in commits.
- Proof is an inspected screenshot, a checked API response or a real UI test, never an HTTP status.
- Secrets are never printed or committed; the repository is public.
- No data about the owner's machine, the fleet or the state of the work in tracked files or agent files: hosts, host paths and local
  checkout hints live in the git-ignored `ay.local.toml` (shape in `ay.local.example.toml`); everything generated (reports, inventories,
  telemetry, research notes, the work ledger, the plan page, learning digests) goes to the git-ignored `.private/` (`tools/local.ts`).
  Docs name hosts by role (`<build-host>`, `<test-host>`, `<prod-host>`). `bun tools/private-gate.ts` enforces it.
