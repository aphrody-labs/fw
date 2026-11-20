---
name: monorepo-dedupe
description: >-
  Expert multi-mega-monorepo deduplication playbook and audit CLI (`monorepo-dedupe-audit.ts`).
  Detects and safely eliminates cross-repo and intra-repo exact SHA-256 duplicates, Cargo/npm
  package collisions, duplicate SKILL.md/plugin trees, and tracked upstream/vendor/fuzz bloat
  without breaking tests, references, or sibling agent sessions.
---

# Multi-Mega-Monorepo Deduplication Expert Playbook

## 1. Pre-Flight Coordination & Ownership

1. Before touching sibling repositories (`$WORKSPACE_ROOT/fw`, `$WORKSPACE_ROOT/aphrody`, `$WORKSPACE_ROOT/ay`, etc.), check `relay` `session_list` + `session_inbox` and register via `session_register` so no active peer session is disrupted.
2. Verify clean `git status --short` and `git branch --show-current` on every target repo before editing.
3. Establish a **single canonical owner** for every shared crate, package, skill, or doc tree before deleting any duplicate copy.

## 2. Four-Layer Forensic Audit (Never Guess)

Run the deterministic Bun audit CLI across all target repos:

```powershell
bun "$HOME/.gemini/scripts/monorepo-dedupe-audit.ts" $WORKSPACE_ROOT/fw $WORKSPACE_ROOT/aphrody $WORKSPACE_ROOT/ay
```

It reports four independent signals from `git ls-files`:
1. **Exact SHA-256 Content Hashes (`>= 256 B`)**:
   - Separates **cross-repo exact duplicates** (`repoA:path == repoB:path`) from **intra-repo duplicates**.
   - Filters out intentional structural duplicates (`LICENSE*`, `COPYING*`, icon style variants, test golden fixtures) before recommending deletion.
2. **Cargo Crate & JS/TS Package Collisions**:
   - Extracts `[package] name` from every tracked `Cargo.toml` and `"name"` from every tracked `package.json`.
   - Flags identical package names appearing in multiple repos or multiple workspace paths (excluding generic `xtask` across independent workspaces).
3. **Skill & Agent Config Collisions**:
   - Extracts `name:` from every `SKILL.md` across `.agents/skills/`, `.agents/plugins/*/skills/`, `.factory/skills/`, `.claude/skills/`, and `plugin/skills/`.
   - When 1 file differs between two copies, inspect `git diff --no-index <copyA> <copyB>` to keep the newer progressive-disclosure version in the canonical plugin path.
4. **Tracked Upstream / Vendor / Fuzz / Local-State Bloat**:
   - Flags tracked `packages/<upstream>/`, `vendor/`, `fuzz/`, `.wrangler/`, `node_modules/`, and large binary blobs.
   - Use `git rm -r --cached <dir>` + `.gitignore` when the local disk checkout must remain usable (e.g. `packages/ghidra`), and `git rm -r` when the duplicate is completely superseded.

## 3. Mandatory Reference-Verification Gate (Before Every Delete)

Never delete a directory on directory-name similarity alone:
1. Run `rg -n "<target-path>" <repo> --glob "!<target-path>/**" --glob "!.git/**"` across every affected repo.
2. Check Cargo workspace `members`/`exclude`, `*.targets.json`, CI workflows, and test files for path assertions.
3. Preserve intentional compatibility paths referenced by live tests or specs (e.g. `ay/.claude/skills` referenced by `packages/ai-os/test/server/e2e.test.ts`).

## 4. Safe Execution & Numerical Proof

1. Update workspace manifests, target registries, `.gitignore`, and tests to assert the deduplicated state (e.g. `expect(existsSync(oldDupPath)).toBe(false)`).
2. Run each modified repo's real verification gates:
   - Rust: `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`
   - TS/Bun: `bun test` + `bun check`
   - Python: `uvx ruff check .`
   - Plugins: `agy plugin validate <dir> --strict` + `bun "$HOME/.gemini/scripts/check.ts"`
3. Re-run `monorepo-dedupe-audit.ts` after cleanup and report exact before/after metrics (`trackedFiles`, `trackedMB`, `crossRepoDupFiles`, `duplicateCrates`, `duplicateSkills`), commit SHA on `main`, and `0 dirty`.
