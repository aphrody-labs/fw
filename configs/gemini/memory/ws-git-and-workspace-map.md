---
name: git-and-workspace-map
description: Checkouts under workspace root, GitHub repos, shared-tree git discipline, relative submodule junctions, and product repository split
type: project
---

- **Active checkouts under `$WORKSPACE_ROOT` (`C:\src` — 6 canonical directories only, zero duplication)**:
  - `aphrody` -> `aphrody-labs/aphrody` (GPU UI library: GPUI, WGPU, Taffy, Native Shell, UI primitives, design tokens; `vendor/` holds `aphrody-git`, `aphrody-labs-github`, `open`, `rust-mono`)
  - `ay` -> `aphrody-labs/ay` (unified `kiri` + `just` task runner, recipe engine, and monorepo build/dev orchestrator + C ABI, corelib, cloud/edge; `third_party/` holds `ay-vercel`, `browser`, `bun`, `rio`)
  - `fw` -> `aphrody-labs/fw` (all OS, Linux, kernel, POSIX, Windows, AI, ML, inference, agents, MCP, ACP, A2A, RAG, `ai-os`)
  - `nie` -> `aphrody-labs/nie` (complete game engine + static/dynamic recompiler: Bevy/WGPU, Steam/Epic launcher, `nie-pe`, `nie-sniff`, `nie-re`, `nie-recomp`, `nie-win32`, `nie-trace`, RE skills)
  - `rosegriffon` -> `rose-griffon/rosegriffon` (full-TypeScript Bun cloud app; Achillea web UI)
  - `shenron` -> `aphrody-labs/shenron`
- **Product & monorepo ownership**:
  - `aphrody.com` (`apps/web`) = marketing/vitrine; `aphrody.app` (`aphrody-app`) = full-Rust apps & Bevy/wgpu game launcher.
  - Submodules use relative URLs and sibling checkouts via directory junctions (never duplicate nested clones).
- **Shared-tree git safety**: always work on `main`; never run `git stash`, `git stash pop`, `git reset --hard`, or branch switches on shared `$WORKSPACE_ROOT` trees; make path-scoped commits and push directly.
- `aphrody-dev` is a user account, not an organization: `orgs/aphrody-dev/...` API routes return 404; use `users/aphrody-dev/repos` or `gh repo list aphrody-dev`. The handle `rosegriffon` no longer resolves on GitHub (the organization is `rose-griffon`).
- rosegriffon moved 2026-11-17 to GitHub org `rose-griffon` (rose-griffon/rosegriffon), owned by the old account `aphrody-code` (token in Windows Credential Manager target `gh:github.com:aphrody-code`; scopes repo, read:org, no admin:org). aphrody-dev has repo admin, not org membership. rosegriffon also has its own Gmail and OVH accounts.
