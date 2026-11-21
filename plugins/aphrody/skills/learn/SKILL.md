---
name: learn
description: "Local autonomous learning in fw: harvest git history, the code graph and docs into one corpus, search it, distill rules (`fw learn`, MCP `fw_learn`)"
---

# Learning loop (one store: fw)

`fw learn` is the only learning store. It harvests git history, the code graph and docs into a sharded local corpus, searches all three, and writes distilled rules; no network service is involved. The former hosted hub (`learn.aphrody.com`) has no writer left and is retired.

## Commands

- `fw learn status`: corpus counts and the dataset root.
- `fw learn harvest`: add git history, the code graph and docs to the corpus.
- `fw learn distill`: write `rules/learned-auto.md` from the corpus.
- `fw learn search <words>`: search first, before re-deriving a known fact. The reply carries `corpusHits` from the docs corpus (`fw-corpus`, skill `corpus`) next to the code and history hits.
- `fw learn loop`: continuous harvest and distill; `fw learn systemd` prints a hardened unit for a VPS (the loop runs on fr1).
- MCP tool `fw_learn` (plugin server `fw`) exposes the same actions.

## Store

Dataset root: `--xfs-root <dir>`, else `FW_LEARN_XFS_ROOT`, else `/srv/xbuild/fw-learn` on a Linux VPS, else `~/.fx/learn-xfs`. Shards are `observations/YYYY-MM.jsonl`, `graphs/<project>/`, `docs/`, `corpus/YYYY-MM.jsonl`, `rules/learned-auto.md`. Claude Code's own memory directory stays the store for user preferences; `fw learn` holds what code and history can show.
