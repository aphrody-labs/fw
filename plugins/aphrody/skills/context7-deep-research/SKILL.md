---
name: context7-deep-research
description: "Research an upstream library, API or tool before using or citing it in src - Context7 docs through its MCP server. Use for unfamiliar APIs, version facts, or checking a doc claim."
---
> live GitHub source through gh api, and the repo itself.


# Context7 and source research

Every fact keeps its source (library id, repo plus commit, file:line, command output) and the date it was read.

## 1. Context7 MCP server

`context7` is a remote MCP server (`https://mcp.context7.com/mcp`, streamable HTTP; anonymous use has a monthly quota, the `CONTEXT7_API_KEY` header lifts it). In Claude Code its tools are `resolve-library-id` and `query-docs` (prefixed `mcp__<server>__` in the tool list). If they are not loaded, add the server with `claude mcp add --transport http context7 https://mcp.context7.com/mcp`.

1. `resolve-library-id` with `{ libraryName, query }` returns ids such as `/oven-sh/bun`.
2. `query-docs` with `{ libraryId, query }` returns the docs.

Without a loaded MCP client, POST JSON-RPC `tools/list` and `tools/call` to the same URL with `content-type: application/json` and `accept: application/json, text/event-stream`.

## 2. Live source, which beats docs

Docs can lag. When the source and the docs disagree, the source wins; report the conflict.

| Question             | Command                                                                                  |
| -------------------- | ---------------------------------------------------------------------------------------- |
| head commit and date | `gh api repos/<o>/<r>/commits/<branch> --jq '.sha[0:10] + " " + .commit.committer.date'` |
| a file at HEAD       | `gh api repos/<o>/<r>/contents/<path> --jq .content`, then base64-decode it in `bun -e`  |
| releases and tags    | `gh api repos/<o>/<r>/releases/latest`, `git ls-remote --tags <url>`                     |
| a crate              | `cargo info <crate>` on fr1                                                              |
| an npm package       | `bun info <pkg>`                                                                         |

Pipe JSON to `jaq` (gh `--jq` breaks on quotes under niubash).

## 3. The repo

- Search with `rg`, `fd` and `ast-grep`.
- Look up symbols with `ay code refs`.
- Read local pins in `src.lock.json`, `DEPS`, `Cargo.lock` and `bun.lock`.
- Look for local checkouts under `third_party/` (untracked, pinned).

## Output

Write the finding next to the work, in the doc or commit that uses it. Only a long survey goes to `docs/research/<topic>-<date>.md`, and it states its sources and date.
