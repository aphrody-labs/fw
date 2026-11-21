---
name: learn
description: Continuous autonomous serverless self-learning unified inside Fw across Antigravity, Codex, and Aphrody Labs agents via Cloudflare (`https://learn.aphrody.com`) and Fw's persistent boot-startup daemon: zero human approval, automatic distillation, promotion, FTS5 indexing, and cross-PC bundle convergence.
---

# Continuous Autonomous Self-Learning Loop (Fw Unified)

Hooks, local distillers, Fw's persistent cross-platform continuous-upgrade daemon (`fw daemon run`), and the Cloudflare `aphrody-learn` Cron worker run completely autonomously with **zero human approval or manual curation**.

## Journal, Local, Fw & Serverless Hub

- Raw events: `~/.gemini/learning/journal.jsonl` (secret-redacted)
- Auto-promoted memory: `~/.gemini/memory/learned-auto.md` & `~/.gemini/memory/learned-cloud.md`
- Indexed stores: `~/.gemini/learning/memory.db` (FTS5)
- Fw target & daemon telemetry: `target/manifest.json`, `target/upgrade-history.jsonl`, `target/daemon-state.json`, `target/daemon.jsonl`
- Serverless hub: `https://learn.aphrody.com` (`/api/learn/events`, `/api/learn/distill`, `/api/learn/manifest.json`, `/api/learn/bundle.json`)

## Commands

- Search memory first: `fw learn search <words>` or `bun ~/.gemini/scripts/learn.ts search <words>` (or `fw_learn` MCP tool).
- Run local distillation: `fw learn distill` or `bun ~/.gemini/scripts/learn.ts distill-quiet`
- Run cloud sync: `fw learn cloud-sync` or `bun ~/.gemini/scripts/learn.ts cloud-sync`
- Run combined Fw upgrade + learning + plugin convergence cycle: `fw daemon run --once --auto-apply --with-learn --sync-plugin`
- Install persistent boot-startup auto-restart loop: `fw service install` (`Windows Task Scheduler`, `Linux systemd --user`, `macOS launchd`).
- Rebuild local rules/plugins/FTS5 index: `bun ~/.gemini/scripts/sync.ts && bun ~/.gemini/scripts/install.ts && bun ~/.gemini/scripts/check.ts`
