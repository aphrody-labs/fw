# Agent frameworks study: OpenClaw, Hermes Agent, and the claude repo

Date 2026-11-17. Sources read live: `aphrody-labs/openclaw` (upstream `openclaw/openclaw`, release `v2026.11.4`, commit `a2a0131d4`) and `aphrody-labs/hermes-agent` (upstream `NousResearch/hermes-agent`, release `v0.23.2`, commit `e22269a1`), shallow clones on fr1 `$WORK/agents`. Machine-readable matrix: `docs/research/agents.toml`. Dependency graph: `docs/graph/*.mmd`. Benchmarks: `docs/bench/*.json`.

## OpenClaw in one page

- TypeScript, Node 24+, pnpm workspace (`extensions/*` about 197, `packages/*`, `apps/{ios,android,macos,linux}`, Rust `crates/`). 886 MB, 51.9k TS files. Config `~/.openclaw/openclaw.json` (JSON5, zod schema in `src/config/zod-schema*.ts`), state in `~/.openclaw/state/openclaw.sqlite`.
- Gateway: one WebSocket server (`src/gateway/`, loopback port 18789, typed frames in `packages/gateway-protocol`). Channels are plugins (`extensions/{whatsapp,telegram,discord,slack,signal,imessage,matrix,msteams,...}`, about 30). Routing by session key `agent:<id>:<channel>:<kind>:<peer>[:thread:<ts>]` (`src/routing/session-key.ts`). DM policy `pairing|allowlist|open|disabled`, pairing codes in sqlite (`src/pairing/`).
- Memory: workspace files `MEMORY.md`, `USER.md`, `memory/YYYY-MM-DD.md`; SQLite FTS plus vector hybrid index (`packages/memory-host-sdk`, `extensions/memory-core`); memory plugin slot (core, lancedb, wiki, active-memory); pluggable context engine for compaction (`src/context-engine/`).
- Skills: `skills/*/SKILL.md` (52) with `metadata.openclaw` install recipes, workspace-first precedence, ClawHub registry, agent-written skills by `extensions/skill-workshop`.
- Tools: exec, files, web, browser, nodes, cron, memory and session tools; MCP both ways (`src/mcp/*.ts` serve channels and tools over stdio); ACP for external coding agents; sandboxes docker, podman, ssh.
- Scheduler: `src/cron/` with schedule kinds `at`, `every`, `cron`, `on-exit`, `stream`; payloads system event, agent message, command, script; delivery to channels or webhooks.
- Self-improvement: skill-workshop review after 10 iterations and 30 s idle, unused-skill archive after 30 days; "dreaming" sweeps (light, REM, deep) with gated promotion to `MEMORY.md` (`extensions/memory-core/src/dreaming-*.ts`).
- Plugins: `extensions/<id>/openclaw.plugin.json` (schema-first `configSchema`, `uiHints`, `activation`), SDK `packages/plugin-sdk`, 14 typed hooks (`src/plugins/hook-types.ts`).

## Hermes Agent in one page

- Python 3.11+, uv, 3.4k source files. Config `~/.hermes/config.yaml`, secrets `~/.hermes/.env`, profiles under `~/.hermes/profiles/`. Entry points `hermes`, `hermes-agent`, `hermes-acp`.
- Gateway: `gateway/` single process, `BasePlatformAdapter`, platform plugins `plugins/platforms/*` (about 26) described by `plugin.yaml` plus `adapter.py` `register(ctx)`. Session key `agent:main:<platform>:<chat_type>[:chat_id][:thread][:user]` (`gateway/session.py`). Pairing in `gateway/pairing.py`.
- Memory: frozen-snapshot `MEMORY.md` and `USER.md` (prefix cache stays valid mid-session), `~/.hermes/state.db` SQLite WAL with FTS5 and trigram (`hermes_state_fts.py`), `session_search` tool, one external memory provider slot.
- Skills: `skills/` (44) and `optional-skills/` (141), hub with ClawHub, GitHub, skills.sh and official sources, lock file, `skills_guard.py` and linter; the agent edits skills through `skill_manage`.
- Scheduler: `cron/jobs.json` per profile, output `cron/output/<id>/<ts>.md`, delivery to any platform.
- Self-improvement: memory nudge every 10 user turns, skill nudge every 15 iterations, a forked background reviewer (`agent/background_review.py`), an idle-triggered curator that archives but never deletes (`agent/curator.py`), trajectory tooling (`batch_runner.py`, `trajectory_compressor.py`).
- Plugins: `VALID_HOOKS` in `hermes_cli/plugins.py` (about 35 events), about 35 model-provider plugins, plugin catalog of 2.4k yaml entries.

## What "the claude repo as an agent" means

Claude Code is the runtime (loop, tools, compaction, subagents, MCP client, plugins, hooks, scheduled cloud routines, channels). The repo adds the layers the two frameworks have and the harness lacks:

| layer                     | OpenClaw / Hermes           | claude repo                                                              |
| ------------------------- | --------------------------- | ------------------------------------------------------------------------ |
| identity and instructions | `AGENTS.md`, `SOUL.md`      | `CLAUDE.md`, fanned out by `scripts/sync.ts`                             |
| durable memory            | `MEMORY.md`, `USER.md`, FTS | `projects/*/memory/`, `packages/aphrody/hooks-handlers/memory.ts` (FTS5) |
| learning loop             | skill-workshop, curator     | `learn.ts` journal, digests, session-start nudge, `memory-curator` job   |
| scheduler                 | `src/cron/`, `cron/`        | `agent.toml` plus `scripts/cron.ts` (one OS tick through `Bun.cron`)     |
| gateway and channels      | `src/gateway/`, `gateway/`  | official channel plugins started by `scripts/gateway.ts`                 |
| plugin and hooks          | manifests and typed hooks   | `.claude-plugin/` manifests, hooks, MCP servers                          |
| portability               | migrate-* importers         | `gemini-extension.json`, `hooks.json` (agy), `scripts/sync.ts`           |

## Matrix

<!-- matrix:start -->

| area | capability | openclaw | hermes | claude repo today |
|---|---|---|---|---|
| gateway | Long-lived multi-channel gateway process | present: `src/gateway/ (WebSocket, loopback :18789, docs/gateway/protocol.md)` | present: `gateway/ (BasePlatformAdapter in gateway/platforms/base.py)` | partial (was missing): `scripts/gateway.ts` - launches one persistent Claude Code session with --channels from agent.toml; no standalone daemon |
| gateway | Messaging channels (Telegram, Discord, WhatsApp, Slack, Signal, iMessage, Matrix, email...) | present: `extensions/{telegram,whatsapp,discord,slack,signal,imessage,matrix,msteams,...} (about 30)` | present: `plugins/platforms/{telegram,discord,whatsapp,slack,signal,matrix,email,sms,...} (about 26)` | partial: `plugins/marketplaces/claude-plugins-official/external_plugins/telegram` - official telegram, discord, imessage, fakechat channel plugins exist; listed in agent.toml, enabled once a bot token is set |
| gateway | DM pairing, allowlists, access policy | present: `src/pairing/, src/security/dm-policy-shared.ts (8-char code, 1 h, max 3 pending)` | present: `gateway/pairing.py (8-char code, 1 h, lockout after 5 failures)` | partial: `plugins/marketplaces/claude-plugins-official/external_plugins/telegram/ACCESS.md` - per-channel pairing and allowlist come from the channel plugin |
| gateway | Session routing keys per channel/peer/thread | present: `src/routing/session-key.ts (agent:<id>:<channel>:<kind>:<peer>[:thread:<ts>])` | present: `gateway/session.py build_session_key` | partial: `packages/aphrody/hooks-handlers/claude-peer.ts` - named live sessions and a mailbox between sessions; no per-peer channel routing |
| gateway | CLI, TUI, web dashboard, native apps | present: `ui/, src/tui/, apps/{ios,android,macos,linux}` | present: `ui-tui/, web/, apps/desktop, acp_adapter/` | present: `scripts/statusline.ts` - Claude Code CLI, IDE and desktop (harness); status line is ours |
| gateway | Voice (STT, TTS, calls, wake word) | present: `extensions/voice-call, extensions/talk-voice` | present: `tools/voice_mode.py, tools/tts_tool_*` | missing: `none` - not wired; Claude Code voice dictation is a harness feature |
| memory | Curated memory files (agent notes + user model) | present: `MEMORY.md, USER.md, memory/YYYY-MM-DD.md in the workspace (docs/concepts/memory-architecture.md)` | present: `tools/memory_tool.py (~/.hermes/memories/MEMORY.md, USER.md, frozen into the prompt at session start)` | present: `projects/C--src/memory/MEMORY.md` - one fact per topic file, merged into SQLite by scripts/merge-brain.ts |
| memory | Full-text memory search (BM25) | present: `packages/memory-host-sdk, extensions/memory-core` | present: `hermes_state_fts.py (FTS5 + trigram)` | present (was missing): `packages/aphrody/hooks-handlers/memory.ts` - FTS5 porter index; CLI scripts/memory.ts; MCP server memory |
| memory | Vector / hybrid memory search | present: `packages/memory-host-sdk/src/engine-*.ts, extensions/memory-lancedb` | partial: `plugins/memory/{byterover,retaindb}` | missing: `none` - FTS5 only; embeddings would add a model dependency |
| memory | Search over past conversations | present: `docs/concepts/session-search.md (sessions_search tool)` | present: `tools/session_search_tool.py` | present (was missing): `scripts/memory.ts` - index --sessions chunks transcript user/assistant text (kind session) |
| memory | Context compaction | present: `src/context-engine/ (before/after_compaction hooks)` | present: `agent/context_compressor*.py (threshold 0.50)` | present: `settings.json` - harness auto-compact, CLAUDE_CODE_AUTO_COMPACT_WINDOW |
| skills | SKILL.md skills with loader | present: `skills/*/SKILL.md (52), src/skills/` | present: `skills/ (44), optional-skills/ (141), tools/skills_tool.py` | present: `packages/aphrody/skills/fleet/SKILL.md` - 41 skills in two plugins plus Anthropic document/example skills |
| skills | Skill registry / hub install | present: `skills/clawhub (ClawHub)` | present: `tools/skills_hub_{clawhub,github,skillssh,official}.py` | partial: `.claude-plugin/marketplace.json` - plugin marketplaces (local, anthropics/skills, claude-plugins-official), not a skill-granular hub |
| skills | Agent writes and maintains its own skills | present: `extensions/skill-workshop (auto/propose, 30-day archive)` | present: `tools/skill_manage, agent/curator.py` | partial: `packages/aphrody/hooks-handlers/learn.ts` - hooks-handlers/learn.ts folds digests into memory; skill-creator comes from example-skills |
| skills | Skill linting, duplicate and safety audit | present: `src/skills/security` | present: `tools/skills_guard.py, skill_linter.py, skills_ast_audit.py` | partial (was missing): `scripts/skills.ts` - frontmatter lint, name collisions, near-duplicate descriptions, listing cost; no content scan |
| tools | Built-in tools (exec, files, web, browser, images) | present: `src/agents/tools/, extensions/browser` | present: `toolsets.py, tools/browser_*` | present: `settings.json` - Claude Code harness tools; browser via claude-in-chrome and chrome-devtools skill |
| tools | MCP client | present: `docs/tools/mcp.md, extensions/mcp-relay` | present: `tools/mcp_tool*.py, optional-mcps/ (66 configs)` | present: `packages/aphrody/.claude-plugin/mcp.json` |
| tools | Agent exposed as MCP server | present: `src/mcp/{channel-server,openclaw-tools-serve,tools-stdio-server}.ts` | partial: `website/docs/reference/cli-commands.md` | present (was partial): `packages/aphrody/hooks-handlers/memory-mcp.ts` - claude-bridge (live sessions) and memory (search) servers on one shared mcp-lite.ts |
| tools | Sandboxed execution backends | present: `src/agents/sandbox/ (docker, podman, ssh, openshell)` | present: `tools/environments/ (local, docker, ssh, modal, daytona, vercel)` | partial: `packages/aphrody/skills/fleet/SKILL.md` - heavy work runs on fr1/fr2 over ssh; no per-session container |
| tools | Subagents and delegation | present: `src/agents/ (sessions_spawn, subagents), docs/tools/subagents` | present: `tools/delegate_tool*.py` | present: `packages/aphrody/agents/worker.md` - worker and researcher agents, spawn depth 5 |
| scheduler | Scheduled jobs (cron, interval, one-shot) | present: `src/cron/ (at, every, cron, on-exit, stream)` | present: `cron/ (jobs.json, output/<id>/<ts>.md)` | partial (was missing): `scripts/cron.ts` - agent.toml jobs (cmd or claude -p), one OS tick through Bun.cron; cloud routines via the schedule skill (harness) |
| scheduler | Deliver job output to a channel | present: `docs/automation/cron-jobs/delivery.md` | present: `cron/scheduler_delivery*.py` | partial (was missing): `packages/aphrody/hooks-handlers/claude-peer.ts` - deliver = inject pushes into live sessions; chat delivery needs an open channel session |
| scheduler | Proactive heartbeat / standing intents | present: `docs/gateway/heartbeat.md, docs/concepts/standing-intents.md` | present: `website/docs/user-guide/features/heartbeat.md` | partial (was missing): `agent.toml` - a cron agent job is the heartbeat; ships disabled (token cost) |
| self-improvement | Capture, review and consolidate learnings automatically | present: `extensions/skill-workshop, extensions/memory-core/src/dreaming-*.ts` | present: `agent/background_review.py, agent/curator.py (nudges every 10 turns / 15 iterations)` | partial: `packages/aphrody/hooks-handlers/learn.ts` - failure/recovery/correction journal, digests, session-start nudge, memory-curator cron (disabled); no forked background reviewer |
| self-improvement | Trajectory export, batch runner, evals | present: `src/trajectory/` | present: `batch_runner.py, trajectory_compressor.py, evals/` | missing: `none` - transcripts are indexed for search only |
| plugins | Plugin manifest and typed lifecycle hooks | present: `extensions/*/openclaw.plugin.json, src/plugins/hook-types.ts (14 hooks)` | present: `plugins/*/plugin.yaml, hermes_cli/plugins.py VALID_HOOKS` | present: `packages/aphrody/.claude-plugin/plugin.json` - Claude Code plugin + hooks; the same handlers also serve agy and gemini |
| plugins | LLM provider abstraction and failover | present: `extensions/* providers (99), docs/concepts/model-failover.md` | present: `plugins/model-providers/ (about 35)` | partial: `settings.json` - Anthropic only, with fallbackModel |
| security | Approvals, untrusted-content handling, secret hygiene | present: `src/security/external-content.ts, src/secrets/, docs/tools/exec-approvals.md` | present: `tools/approval*.py, tools/skills_guard.py, cron/prompt_scan.py` | partial: `packages/aphrody/hooks-handlers/lib.ts` - secret redaction in the journal, security-guidance plugin; approvals are off by owner policy |
| portability | Import/export between agent frameworks and CLIs | present: `extensions/migrate-hermes, extensions/migrate-claude` | present: `website/docs/user-guide/import-from-other-agents.md` | partial: `scripts/sync.ts` - instructions and memory fan out to gemini, codex, opencode, pi; agy and gemini load the same plugins |

Totals over 29 capabilities (present / partial / missing): openclaw 29/0/0, hermes 27/2/0, claude 11/15/3.

Closed or improved on the claude side by this work (8): gateway (missing to partial), memory-fts (missing to present), session-search (missing to present), skill-audit (missing to partial), mcp-server (partial to present), cron (missing to partial), delivery (missing to partial), heartbeat (missing to partial).

<!-- matrix:end -->

## Reverse-engineering notes

Upstream behaviors worth keeping (paths in the upstream checkouts):

1. Hermes memory is frozen into the system prompt at session start; writes mid-session reach disk only, so the prefix cache is never invalidated (`tools/memory_tool.py`). The same rule applies to Claude Code: memory edits take effect next session, and `CLAUDE.md` should not be rewritten per turn.
2. Hermes curator is inactivity-triggered with state in `.curator_state`, archives and never deletes (`agent/curator.py`). Our `memory-curator` job follows the same rule through the learn skill.
3. OpenClaw writes `MEMORY.md` only through gated consolidation, so untrusted content never reaches the curated core (`src/security/external-content.ts` marks external text).
4. Both use the `agent:main` session-key prefix; a Hermes profile literally named `main` becomes `main~` (`gateway/session.py`). Keys are interchangeable between the two.
5. OpenClaw plugins are schema-first: `configSchema` and `uiHints` in the manifest drive the settings UI (`extensions/telegram/openclaw.plugin.json`).
6. Webhook-originated Hermes sessions get a four-tool allowlist because payloads are untrusted (`toolsets.py` `_HERMES_WEBHOOK_SAFE_TOOLS`).

Behaviors found in this repo while reverse-engineering the Claude Code side (fixed in this change):

1. `bun check <file>` runs the project's `check` script when `package.json` has one. The aphrody `typecheck` PostToolUse hook therefore ran the whole `scripts/check.ts` gate (marketplace validation, lint, hook smoke tests) after every TypeScript edit: 3088 ms per edit instead of 56 ms. Fix: `bun check --no-script` (flag listed in `bun check --help`).
2. Claude Code reports failed tools through the `PostToolUseFailure` event, not through `PostToolUse`. The learn hook matched error words in successful results, so any file or log containing `error:` was journaled as a failure (digests were noise). Fix: failures come from `PostToolUseFailure` only; text matching stays for harnesses without that event.
3. YAML frontmatter with `: ` inside an unquoted `description` is invalid YAML. Claude Code loads it leniently; strict loaders (agy `--strict`, gemini) do not. Four aphrody skills were affected and are now quoted (`scripts/skills.ts` lints this).
4. Directory-marketplace plugins are read from their folder (`claude plugin update` says so), edits apply at the next session start or `/reload-plugins`. `plugins/cache/aphrody/*` is a stale copy from the first install (old MCP set, the 900-tool monolithic `ovhcloud` server); it is not read and was kept only because `installed_plugins.json` still references it.
5. Two bun-plugin hooks (`post-tool-use.ts`, `user-prompt-submit.ts`) only wrote `{}`: two process spawns per tool call and per prompt for nothing.

## Craft Anything

Each column maps onto the craft loop (study the target, build a clean-room version, check against the original, refine); see `[craft.*]` in `agents.toml`. Not built here.

## Remaining

- vector or hybrid memory search (needs an embedding model; FTS5 only today);
- voice, trajectory export and batch evals;
- a forked background reviewer (Hermes `agent/background_review.py`): the learn loop needs a model call per review, so it ships as the disabled `memory-curator` job;
- channel tokens (Telegram, Discord, iMessage bot setup is a human step per plugin README) and `[gateway].channels` stays empty until then;
- OpenClaw's stdio MCP server (`src/mcp/channel-server.ts`) as a WhatsApp/Signal channel source: needs a running OpenClaw gateway on fr1 and credentials;
- craft jobs for the three columns (specified in `agents.toml`, not built).
