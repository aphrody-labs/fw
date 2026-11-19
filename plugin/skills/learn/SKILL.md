---
name: learn
description: Continuous autonomous serverless self-learning unified inside Fairwind across Antigravity, Codex, and Aphrody Labs agents via Cloudflare (`https://learn.aphrody.com`) and Fairwind's persistent boot-startup daemon: zero human approval, automatic distillation, promotion, FTS5 indexing, and cross-PC bundle convergence.
---

# Continuous Autonomous Self-Learning Loop (Fairwind Unified)

Hooks, local distillers, Fairwind's persistent cross-platform continuous-upgrade daemon (`fairwind daemon run`), and the Cloudflare `aphrody-learn` Cron worker run completely autonomously with **zero human approval or manual curation**.

## Journal, Local, Fairwind & Serverless Hub

- Raw events: `~/.gemini/learning/journal.jsonl` (secret-redacted)
- Auto-promoted memory: `~/.gemini/memory/learned-auto.md` & `~/.gemini/memory/learned-cloud.md`
- Indexed stores: `~/.gemini/learning/memory.db` (FTS5)
- Fairwind target & daemon telemetry: `target/manifest.json`, `target/upgrade-history.jsonl`, `target/daemon-state.json`, `target/daemon.jsonl`
- Serverless hub: `https://learn.aphrody.com` (`/api/learn/events`, `/api/learn/distill`, `/api/learn/manifest.json`, `/api/learn/bundle.json`)

## Commands

- Search memory first: `fairwind learn search <words>` or `bun ~/.gemini/scripts/learn.ts search <words>` (or `fairwind_learn` MCP tool).
- Run local distillation: `fairwind learn distill` or `bun ~/.gemini/scripts/learn.ts distill-quiet`
- Run cloud sync: `fairwind learn cloud-sync` or `bun ~/.gemini/scripts/learn.ts cloud-sync`
- Run combined Fairwind upgrade + learning + plugin convergence cycle: `fairwind daemon run --once --auto-apply --with-learn --sync-plugin`
- Install persistent boot-startup auto-restart loop: `fairwind service install` (`Windows Task Scheduler`, `Linux systemd --user`, `macOS launchd`).
- Rebuild local rules/plugins/FTS5 index: `bun ~/.gemini/scripts/sync.ts && bun ~/.gemini/scripts/install.ts && bun ~/.gemini/scripts/check.ts`
