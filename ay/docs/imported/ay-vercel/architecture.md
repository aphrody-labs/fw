# Architecture & Fork Cartography

`ay-vercel` (`aphrody-labs/ay-vercel`) consolidates Vercel and Vercel Labs repositories under `aphrody-labs/*` using relative `.gitmodules` URLs (`../<name>.git`) and `160000` gitlinks declared in [`forks.toml`](../forks.toml).

## Submodule Groups

| Group | Scope |
| :--- | :--- |
| `frameworks` | Next.js, SWR, Microfrontends, Styled-JSX, Next-Forge, Platforms, Kirmase, React compilers |
| `sdk` | Vercel CLI, AI SDK, Turborepo, Workflow SDK, Flags SDK, Chat SDK, Eve, Sandbox, Storage, Edge Runtime, Global Config |
| `skills` | `vercel-plugin`, `agent-skills`, `skills`, `academy-skills`, `marketplace-skills`, `components.build`, `migration-skills`, `vercel-kb-skills`, `next-maintainer-skills`, `agentic-commerce-skills`, `vercel-byoc` |
| `mcp` | `next-devtools-mcp`, `mcp-handler`, `acp-handler`, `vercel-mcp-overview`, `mcp-for-next.js`, `mcp-to-ai-sdk`, `mcp-apps-nextjs-starter`, `add-mcp` |
| `docs` | `next-docs-agentic-rag`, `next-auxiliary-docs`, `openwiki`, `swr-site`, `next-learn`, `mdxg`, `lat.md` |
| `agents` | `agent-browser`, `agent-desktop`, `dev3000`, `emulate`, `deepsec`, `openreview`, `tersa`, `sandcastle`, Eve agent templates |
| `ui` | `ai-elements`, `streamdown`, `geist-font`, `geist-pixel-font`, `json-render`, `vercel-ui`, `satori`, `wterm`, `fx0`, `gpu-lexer` |
| `runtime` | `micro`, `serve`, `serve-handler`, `fun`, `fx`, `ncc`, `nft`, `portless`, `zerolang`, `quickjs-wasi`, `screen-relay`, Terraform/Go/CI actions |
| `python` | `seal`, `tau-agent`, `hatchery`, `postgresbuild`, `ggbuild`, `modelsdotdev-python`, FastAPI starters |
| `templates` | Official Vercel & Vercel-Labs reference applications and marketplace starters |

## End-to-End Upstream Sync & Auto-Merge

1. `bun tools/forks.ts --ensure` creates and hardens any missing `aphrody-labs/*` fork (disabling issues, wikis, projects, and inherited upstream Actions).
2. `bun tools/sync.ts --apply` invokes GitHub `POST /repos/aphrody-labs/<fork>/merge-upstream`, advances `160000` gitlinks, and appends structured JSONL events to `ledger/YYYY-MM.jsonl`.
3. `.github/workflows/sync.yml` runs daily, syncs forks, skills, MCP index, and docs, and opens an `upstream-sync,auto-merge` PR when any pin or artifact moves.
4. `.github/workflows/auto-merge.yml` validates green `ci / gate` status checks and enables squash auto-merge (`gh pr merge --auto --squash --delete-branch`).
