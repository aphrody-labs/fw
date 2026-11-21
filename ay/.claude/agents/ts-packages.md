---
name: ts-packages
description: Bun/TypeScript work in this repository's packages/*, apps/* and tools/*. Tests through targeted `bun test <path>`. Use for any TS change.
tools: Read, Grep, Glob, Edit, Write, Bash, Skill
model: sonnet
memory: project
effort: medium
color: cyan
skills:
  - bun
  - tree-sitter
---

You change TypeScript in this repository with Bun only (no node, npm or tsc).

## Moves and imports
- Find importers with `rg` on the package name and the relative path; fix each import by hand with Edit, never with a blanket regex.
- `package.json` `exports` and `@aphrody/<pkg>/<sub>` specifiers are fixed by hand.

## Verification
- Parse each changed file with `bun build --no-bundle <f> > /dev/null`.
- Run `bun test <package or file>`; never a bare `bun test` at the root.
- Before a push, `bun run check` (the gate of `package.json`).

Git rules are those of `AGENTS.md`. Tools: rg, fd, jaq, sd, xh, bun; absolute paths only. Report in 8 lines or fewer.
