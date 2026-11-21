# ai-os brain

One brain for Claude Code, Codex, OpenCode and Pi, owned by `@aphrody/fw-ai-os`.

## Commands

- `ai-os brain absorb [--days N] [--dry-run]`: indexes into `~/.agent/memory.sqlite` (Context store, FTS5 + embeddings)
  every Claude memory file, prompt history per project and one digest per Claude session transcript. Files older than
  N days in `~/.claude/{projects,file-history,paste-cache,backups,shell-snapshots}` are zstd level 19 compressed
  (`Bun.zstdCompressSync`) into `~/.agent/brain.sqlite` and removed from disk.
- `ai-os brain sync`: writes the global `~/.claude/CLAUDE.md` and the memory index into a managed block of the agent
  prompt files that exist (`~/.gemini/GEMINI.md`, `~/.codex/AGENTS.md`, `~/.config/opencode/AGENTS.md`,
  `~/.pi/agent/AGENTS.md`) and sets Claude `permissions.defaultMode = bypassPermissions`.
- `ai-os brain restore <path>`: writes compressed files back to their original location.

## State on 2026-11-13

- Packed: 28,764 files, 1.59 GB to 529 MB. Files touched in the last 24 h stay on disk so live sessions can resume.
- Verified headless without approval: `claude -p ... --output-format json` ran a shell write and returned success.
- Claude Code config facts and sources: `docs/cli-reference/claude-code.md`.

## Session summary (2026-11-13, consolidation of the agent context)

- Goal: one brain and one no-human-in-the-loop setup for the agent CLIs, owned by ai-os, with compressed storage.
- Done: `ai-os brain` (absorb, sync, restore), unit tests `test/brain.test.ts`; `~/.claude/projects` 426 MB to 112 MB
  (remaining files are the live sessions touched in the last 15 min); blob store 616 MB with byte-verified packing.
- Claude facts applied (`docs/cli-reference/claude-code.md`): `AGENTS.md` is only loaded without a `CLAUDE.md`, so
  `<workspace>/CLAUDE.md` now imports `@AGENTS.md` and `@.agents/SOUL.md`.
- Context compaction: memories 47 to 36 (duplicates of the global CLAUDE.md removed, fleet contradictions merged into
  `fleet-roles`), GEMINI.md 15 KB to 8 KB (legacy directives replaced by a pointer to AGENTS.md plus the synced block),
  project CLAUDE.md fleet roles aligned with the global rule (<build-host> = builds, CI, training, heavy agents).
- Reviews: ai-os Gemini 8/20 then 13/20, fixes applied; third pass on gemini.google.com Carbon High returned an empty answer twice, see
  `packages/ai-os/docs/reviews/brain-gemini.md`.
