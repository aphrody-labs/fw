# Research: CLI-Anything, ECC, and claude-mem (2026-11-20)

Live snapshot fetched via `gh api` on `2026-11-20`.

## 1. HKUDS/CLI-Anything (`v0.4.0`, 61,117 stars)
- **Core idea**: Wraps desktop/GUI/web software into deterministic, agent-native CLIs (`<app> --json`, REPL, subcommands, schema-driven discovery) published via `registry.json` and `matrix_registry.json`.
- **Multi-agent packaging**: Ships `.claude-plugin/`, `.cursor-plugin/`, `.pi-extension/`, `codex-skill/`, `hermes-skill/`, `opencode-commands/`, and `skill_generation/` across 75+ target apps (Blender, GIMP, Audacity, QGIS, Godot, Kdenlive, OBS, LibreOffice, RenderDoc, etc.).
- **Takeaways for `fw`**:
  - Align `fw-hermes-claw` CLI-to-Skill / CLI-to-MCP generation with `CLI-Anything`'s `--json` contract and `registry.json` index.
  - Keep `skill_generation` deterministic so any new `fw` subcommand or external app wrapper exports a valid `SKILL.md` + MCP schema automatically.

## 2. affaan-m/ECC (`v2.2.3`, 307,644 stars)
- **Core idea**: Harness performance optimization system for coding agents — `skills/`, `agents/`, `hooks/`, `rules/`, `contexts/`, `scaffolds/`, `manifests/`, `mcp-configs/`, and `ecc2/` runtime.
- **Multi-harness distribution**: Targets, among others, `.claude-plugin/`, `.codex-plugin/`, `.cursor/`, `.gemini/`, `.hermes/`, `.openclaw/`, `.opencode/`, `.zed/`, `install.ps1`, and `install.sh` from one shared source tree.
- **Takeaways for `fw`**:
  - Use `fw/configs/{gemini,claude}` as the single source of truth for rules, hooks, skills, and manifests, mirroring ECC's `install.ps1` / `install.sh` + `manifests/` pattern.
  - Adopt ECC's instinct/memory distillation and security-guide checks inside `fw` CI gates.

## 3. thedotmack/claude-mem (`v16.0.1`, 118,571 stars)
- **Core idea**: Persistent cross-session memory daemon (`services/`, `src/`, `plugin/`, TypeScript/Bun) capturing tool observations, compressing them with AI, and injecting relevant context on session start.
- **Multi-agent & vector stack**: Supports Claude Code, OpenClaw, Codex, Gemini, Hermes, Copilot, OpenCode (`hermes/`, `openclaw/`, `omp/`, `docker/`, `bunfig.toml`) with Chroma/SQLite hybrid retrieval and worker HTTP routes.
- **Takeaways for `fw`**:
  - `fw mem` (keyless 3-layer BM25 + SQLite FTS5) can adopt `claude-mem`'s session-hook capture + progressive-disclosure injection (`index -> timeline -> full observation`) without external daemon lock-in.
  - Track `claude-mem` worker lifecycle fixes (terminal 404/error handler ordering, collection reconciliation) in `fw-hermes-claw` and `fw-rag`.
