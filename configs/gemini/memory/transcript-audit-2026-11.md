# Cross-Transcript Forensic Audit (2026-11-19)

Scanned **111** transcripts (**53,069** steps) in `~/.gemini/antigravity-cli/brain/*/logs/transcript.jsonl`:
- **2,469** error steps (4.65%) and **85** explicit user corrections.
- **71/111** sessions (64%) had **0** `read_file` calls on any `SKILL.md`.
- Top failure categories:
  - Shell / PowerShell 5.1 / CLI quoting & `jq`: **1,022** (`849` non-zero + `156` ParserError + `17` jq)
  - Rust compile / Clippy: **320**
  - `edit_file` target mismatch / `write_file` overwrite: **185** (`173` + `12`)
  - Auth/API scope (`401`/`403`): **149**
  - HTTP/API `404 Not Found` (unverified URLs/endpoints): **109**
  - TypeScript / Bun type/syntax: **72**
  - Missing FS paths (`ENOENT`): **61**
  - SQLite lock / unfinalized statements: **44**
  - Hung commands / timeouts: **38**
  - Windows DLL lock (`os error 5`): **6**

## Radical Patches Applied
1. Purged 12 noisy legacy `[recovery] Recovered from [edit_file]` lines from `memory/learned-auto.md` and replaced with 11 sharp, root-cause guardrails covering all 8 failure modes.
2. Added mechanical Windows `curl` -> `curl.exe` auto-rewrite in `scripts/rewrite.ts` (`PreToolUse` hook via `fast-exec.ts`).
3. Hardened `rules/global.md` pre-flight gates (Initial Hypothesis Grounding + Zero False Completion + Exact-Read-Before-Edit + PowerShell 5.1 safety).
4. Re-synced `GEMINI.md`, all 4 plugins, cross-agent exports, and FTS5 `memory.db` via `sync.ts` + `install.ts` + `check.ts`.
