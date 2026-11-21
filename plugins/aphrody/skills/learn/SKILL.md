---
name: learn
description: "Local autonomous learning in fw: harvest git history, manifests, docs, code graph, CI runs, releases, issues and pull requests of every product into one store, search it, distill rules and publish them to AGENTS.md and llms.txt (`fw learn`, MCP `fw_learn`)"
---

# Learning loop (one store: fw)

`fw learn` is the only learning store. It reads every product of the repository, keeps the result in one SQLite FTS5 database, searches it, distills rules from it, and writes those rules where agents read them. No network service is involved except GitHub (through `gh`) and the release manifest. The engine is the `fw-learn` binary next to `fw` (`crates/fw-learn`).

## Commands

- `fw learn status`: item counts by kind and product, commits, units, CI runs, rules, the harvested head of each repository.
- `fw learn harvest [--repo DIR | --repos DIR] [--no-graph] [--no-remote]`: read git history (incremental), unit manifests, Markdown, the code graph of each product (`fw-graph`), CI runs, the release manifest, issues and pull requests.
- `fw learn distill`: turn the corpus into rules: standing rules from AGENTS.md and CLAUDE.md, pitfalls (areas with the most fix commits), hot areas, files that change together, architecture, CI failure rates, releases, open issues, commit style. Every rule carries its evidence.
- `fw learn search <words> [--kind K] [--product P] [--limit N]`: search first, before re-deriving a known fact. Kinds: `rule`, `rule-source`, `doc`, `commit`, `unit`, `ci`, `release`, `issue`, `pr`, `graph`. The reply carries `codeGraphHits` and `corpusHits` (docs corpus `fw-corpus`, skill `corpus`) next to the store hits.
- `fw learn rules [--product P] [--full]`: the distilled rules as Markdown.
- `fw learn publish [--dry-run]`: commit, by path through a temporary index onto origin/main, a generated block in each product's AGENTS.md (and CLAUDE.md when it exists), `docs/knowledge/learned/<product>.md`, `llms.txt` and `llms-full.txt`. The generated blocks are fenced by `learn:begin` and `learn:end` comments; edit around them, never inside.
- `fw learn loop`: harvest, distill and publish on an interval; `fw learn systemd` prints a hardened unit for a build host.
- MCP tool `fw_learn` (plugin server `fw`) exposes the same actions; `publish` is a dry run unless `push` is true.

## Store

Dataset root: `--root <dir>`, else `FW_LEARN_XFS_ROOT`, else `/srv/xbuild/fw-learn` on a build host, else `~/.fx/learn-xfs`. `learn.db` holds everything; `graphs/<repo>-<product>/` holds the code graphs. Records of the first engine (`observations/`, `corpus/`, `docs/`) are imported once. Text is scrubbed of emails, home paths, IP addresses and tokens before it is stored. Claude Code's own memory directory stays the store for user preferences; `fw learn` holds what code and history can show.
