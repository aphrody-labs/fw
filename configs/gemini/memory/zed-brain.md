# Zed Real-Time Ecosystem Knowledge Digest

Authoritative live-synced Zed references live in `~/.gemini/brain/zed/` (`INDEX.md`, `LIVE_SNAPSHOT.md`, `ZED_DEV.md`), `~/.gemini/brain/zed-ai/`, `~/.gemini/brain/zed-nix/`, and `~/.gemini/plugins/aphrody/docs/zed/{ai,nix}/`, all FTS5-indexed in `~/.gemini/learning/memory.db`.

## Live Sources & Refresh

- Refresh all live Zed GitHub & `zed.dev` sources on demand:
  `bun ~/.gemini/scripts/import-zed-brain.ts --live --sync`
- Covers in real time:
  - GitHub `zed-industries/zed`: Issues, Pull Requests, Discussions, Actions Workflows, Releases, Security Advisories (GHSAs), Insights/Pulse, and `orgs/zed-industries/projects/78` ("Zed Community Requests").
  - `zed.dev`: `/blog` (Delta, Xanadu, Sandboxing, Hidden Gems, Zed Guild), `/about`, `/roadmap`, `/theme-builder`, `/languages` (75+ indexed language slugs).
  - Upstream docs mirrors: `docs/src/ai` (28 files -> `zed-ai` skill) and `nix/` + `flake.nix` (14 files -> `zed-nix` skill).
