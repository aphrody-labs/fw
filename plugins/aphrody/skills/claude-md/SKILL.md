---
name: claude-md
description: "Audit and improve CLAUDE.md files, and capture session learnings into them."
---
# CLAUDE.md maintenance

`claude-md-improver` audits and rewrites; `revise-claude-md` records what this session learned.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/claude-md-improver.md](references/claude-md-improver.md) | Audit and improve CLAUDE.md files in repositories. Use when user asks to check, audit, update, improve, or fix CLAUDE.md files. Scans for all CLAUDE.md files, evaluates quality against templates |
| [references/revise-claude-md.md](references/revise-claude-md.md) | Update CLAUDE.md with learnings from this session |

<!-- merge-skill-families:v1 -->
