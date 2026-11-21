# modern-web

Agent plugin (Claude Code and agy) whose skills are generated from upstream sources, never hand-written, and refreshed by a scheduled sync. Every indexed entry carries a citation: `repo@commit path:line` for repositories, `spec URL (date)` for specifications.

| Skill | Sources |
| --- | --- |
| `bun-reference` | oven-sh/bun `docs/` (docs, guides, blog and release notes) and `packages/bun-types`, aphrody-labs/bun pages and types that differ, `bun --help` for every command, GitHub releases of both |
| `modern-web-guidance` | GoogleChrome/modern-web-guidance-src `guides/` and its SKILL.md, run through `bunx modern-web-guidance@latest`; supersedes the separately installed `~/.claude/skills/modern-web-guidance` and the former hand-written `aphrody:modern-web-guidance` |
| `chrome-extensions` | modern-web-guidance-src `skills-src/chrome-extensions` |
| `webgpu-spec` | gpuweb/gpuweb (WebGPU, WGSL), gfx-rs/wgpu (changelog, docs, public API), webref sections and definitions |
| `webassembly-spec` | WebAssembly/spec (core, JS API, Web API), WebAssembly proposals in webref, WebAssembly/WASI WIT, wasm-bindgen guide |
| `web-specs` | every WHATWG standard plus the W3C specs that Modern Web Guidance (web-features `spec`) or Bun docs link to, from w3c/webref and the spec source repositories |

## Layout

- `scripts/sources.ts`: the upstream list (repository, sparse paths, role).
- `scripts/sync.ts`: shallow, blobless, sparse checkouts into `$MODERN_WEB_CACHE` (default `~/.cache/modern-web`), parsers for MDX, Markdown, Bikeshed, Wattsi, RST, `.d.ts`, Rust doc comments and WIT, then writes `skills/` and `modern-web.lock.json`. Each generated skill carries its own index (`data/<domain>.jsonl`) and `scripts/query.ts`, so it keeps working when copied into another plugin (plugins/aphrody absorbs these skills through `unify.ts`).
- `scripts/query.ts`: FTS5 search (bun:sqlite) over the skill's `data/` (or every skill's from the plugin root, `--domain` to filter); `--full` prints the section text from the cached checkout or `raw.githubusercontent.com` at the pinned commit.
- `modern-web.lock.json`: pinned commit and date of every upstream, fork ahead/behind counts, webref crawl date, npm versions, entry counts.

## Sync

```sh
bun scripts/sync.ts            # fetch every upstream, regenerate
bun scripts/sync.ts --offline  # regenerate from the cache only
bun scripts/query.ts "canvas context sizing" --domain webgpu --full
```

Schedule: GitHub Actions cron in this repository, [`.github/workflows/modern-web-sync.yml`](../../.github/workflows/modern-web-sync.yml), daily at 04:41 UTC (plus `workflow_dispatch`). It runs the sync with the latest Bun release, smoke-tests two queries and commits `skills/` and the lock file to `main` as `fw-sync`. CLI help in `bun-reference` is captured from the Bun that ran the sync; its revision is in the lock file (`bunCli`).

After a sync lands locally: `claude plugin marketplace update fw`, `claude plugin update modern-web@fw`, then `bun ~/.gemini/scripts/install.ts` for agy.

## Privacy

Runs locally or in GitHub Actions; reads public repositories, the npm registry and jsDelivr; sends no workspace data anywhere.
