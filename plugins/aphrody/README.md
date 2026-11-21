# aphrody

The single plugin of the `fw` marketplace, for Claude Code. Everything the fleet used to ship as `fw`, `bun` and a dozen third-party plugins is regenerated into this one tree from `sources.toml`.

- `skills/`: one deduplicated skill set (native skills plus families merged from upstreams: `stitch`, `android`, `android-compose`, `android-ai`, `huggingface`, `huggingface-sagemaker`, `gemini-api`, `modernize`, `refactor`, `claude-md`, `agent-desktop`).
- `agents/`: `worker`, `researcher` and the code-modernization specialists. `output-styles/autonome.md`: the Autonome output style.
- `hooks/`: `pre-tool-use.ts` rewrites legacy shell commands (npm, npx, yarn, pnpm, node, eslint, prettier) to `bun`, `bunx`, `bun lint`, `bun fmt` transparently; `auto-update.ts` (SessionStart) only stats a stamp file and detaches the update; `session-start.ts` emits the Bun session context. All hooks read the Claude Code hook JSON on stdin.
- `mcp/fw-mcp.ts`: the `fw` MCP server. `.claude-plugin/mcp.json` also carries the Hugging Face MCP; `.lsp.json` carries rust-analyzer and kotlin-lsp.
- `sources.toml`, `sources.lock.json`, `THIRD_PARTY.md`, `licenses/`: every absorbed source with its pinned commit and license.

Toolchain in vendored content: `npx` becomes `bunx`, `npm install|run` becomes `bun add|run`, `node f.js` becomes `bun f.js`, `pip install` becomes `uv pip install`, `python -m venv` becomes `uv venv`, `python x.py` becomes `uv run x.py`, jest/vitest/mocha become `bun test`, eslint becomes `bun lint`, prettier becomes `bun fmt`.

## Regenerate and update

```text
bun plugins/aphrody/scripts/unify.ts --update   # moves every ref to its head, rewrites the managed files and the lock
bun plugins/aphrody/scripts/unify.ts            # replays the locked commits (idempotent)
bun plugins/aphrody/scripts/unify.ts --check    # exit 1 when the tree differs from what the generator produces
bun test ./plugins/aphrody/scripts
```

`.github/workflows/unify-plugin.yml` runs `--update` every 6 hours and commits to `main`. The plugin version is `<base>-<content hash>`, so a content change is always a new version.

```text
claude plugin marketplace update fw && claude plugin update aphrody@fw   # the SessionStart hook does this at most every 6 h
```

## Compute targets

the equal cluster nodes `n1` and `n2` (`cluster/build.toml`; builds, CI, agents and services on both, placed by live load with `bun configs/claude/scripts/build-cluster.ts place`), gcloud Cloud Build, Cloudflare Workers, R2 and containers, and the local RTX 4070 for GPU work. Cross-session delivery between live Claude Code sessions is the `claude_inject` / `claude_sessions` MCP tools and `bun ~/.claude/scripts/notify.ts`.

Left out on purpose: `plugins/{hermes,vllm,llmd,openclaw}-fw` are runtime adapters for other products (tests import them), not Claude content. `~/.claude/skills/linux-kernel-docs` (42 MB of generated pages) and the desktop-synced skills stay where they are.
