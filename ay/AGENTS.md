# AGENTS.md

Contract for every agent working in this repository. Names come from `ay.toml`.

## Scope
- This repository is the SDK of Aphrody: C ABI, OpenAPI, docs, spec, standards, corelib, libc and the
  design system. The complete OS, in pure Rust, is `aphrody-dev/aphrody`, the source of truth; this SDK
  follows it. Satellites (`ay.toml` [repos], [registries]) build on both.
- The products and their definition of done are in `docs/spec.md`; the tree and toolchain in
  `docs/architecture.md` and `docs/adr/`; the full scope, row by row, in `docs/features.toml`.
- A feature is done only with a proof in the matrix (test id, screenshot path or artifact URL);
  `bun run check` refuses anything else.

## Rules
- No prototypes, scaffolds or placeholders: add a folder only with a working, tested unit.
- Follow the tree in `docs/architecture.md`: products in `apps/`, Rust in `crates/`, Bun packages in
  `packages/`, the distribution in `os/`, upstreams as submodules in `third_party/`.
- Assemble upstream first; never copy an upstream's code or directory layout into the tree.
- Nothing hardcoded: names, hosts and domains come from `ay.toml`; versions are read live.
- Run the repository's own checks before every push (`bun run check`, and the cargo checks once crates exist).
- Heavy builds run on the `build` host, tests on `test`, production only on `prod` (`ay.toml` [hosts]).
- Work on `main`; no branches, worktrees, stash or reset. No AI attribution in commits.
- Proof is an inspected screenshot, a checked API response or a real UI test, never an HTTP status.
- Secrets are never printed or committed; the repository is public.
