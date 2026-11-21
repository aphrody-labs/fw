# agy-retirement: remove the retired Antigravity surfaces from fw and the live config (queued)

Phase 0. Owner: the Claude Desktop session "Audit et déploiement du framework" (it owns `C:\src\fw`). Release v2026.11.0: not blocking; do it before the plugin freeze so the release ships no dead rules.

## Goal
agy (Antigravity CLI) was killed on 2026-11-21 and its repo is archived. Everything in fw that installs, documents or relies on it, and every rule that contradicts the current stack, is removed or regenerated from code.

## Facts checked 2026-11-21 (origin/main of fw)
- `configs/gemini/` holds 106 files (memory 46, scripts 40, dashboard 6, rules 5, `mcp_config.json`, `hooks.json`): the agy config source.
- 13 files under `configs/gemini` cite the relay hub and 12 cite fr2. Outside it, 40 files in `plugins`, `configs`, `tools` and `docs` mention the relay (27 outside `configs/gemini`) and 23 mention fr2 (11 outside `configs/gemini`), among them `plugins/aphrody/skills/fleet/SKILL.md`, `plugins/aphrody/skills/ai-os-orchestrator/SKILL.md`, `plugins/aphrody/rules/AGENTS.md`, `plugins/aphrody/README.md`, `docs/architecture/os-strategy-2026.md`.
- The skill `plugins/aphrody/skills/agy-knowledge` is a verified-operations spec for a tool that is gone; its `references/agy-knowledge.verified.json` carries model ids (`gemini-4-neon`) that no fw code checks against the live API.
- `cluster/cloudflare-gcloud.toml` (committed 2026-11-19, during the fairwind period of agy conversation 72d50cfe) names `decisions_model = "gemini-decision-1"` and an endpoint `.../v1beta/decisions`; `crates/fw-proto` and `crates/fw-wasm` repeat `gemini-decision-1`. No source in the repo or this review shows the model or endpoint exists.
- `https://learn.aphrody.com` answers with JSON `service: aphrody-learn`, `revision: 21`, 6 rules, 16 memory topics, 1 pc, 4 harnesses (read 2026-11-21): the agy learning loop still runs in production. `plugins/aphrody/skills/learn/SKILL.md` and `configs/gemini/scripts/learn.ts` point to it.
- The live global `CLAUDE.md` still says "`fr2` tests/bench/research" and "sync agy via `bun ~/.gemini/scripts/install.ts`", while its fw source `configs/claude/CLAUDE.md` already dropped the fr2 role: the live copy is behind its source.

## Do
1. Delete `configs/gemini/` and the agy install and sync steps (`configs/gemini/scripts/install.ts` and `sync.ts`, the agy targets in `plugins/aphrody/scripts/unify.ts` and `sources.toml`, the global-rules text) once nothing reads them; keep `plugins/aphrody` as the single plugin source for Claude only.
2. Remove or rewrite the skill `agy-knowledge`; keep only facts that survive a live check (ACP, MCP spec 2026-07-28, 2026-11 CLI behaviour) in `docs/research/`.
3. Replace every relay and fr2 sentence in the skills and docs listed above with the current mechanism (`claude_inject`, `notify.ts`, fr1 and prd) at the source, then regenerate the mirrors; add a `--check` that fails on the words relay and fr2 outside `docs/agents/queue/` and this lot's history.
4. Decide `learn.aphrody.com`: repoint it to `fw learn` (one store) or retire the Worker; either way remove the agy harness. Measure what posts to it first (revision 21, 1 pc).
5. Verify `gemini-decision-1`, `gemini-4-neon` and `v1beta/decisions` against the live Gemini API model list; delete what does not exist.
6. Re-sync the live global `CLAUDE.md` from `configs/claude/CLAUDE.md` with the repo's sync script.

## Acceptance
`git grep -n -i -e relay -e '\bfr2\b' -e antigravity` on fw is empty outside the history notes; `bun configs/claude/scripts/check.ts` passes; `learn.aphrody.com` answers as decided; a fresh Claude session loads the plugin without a gemini path.
