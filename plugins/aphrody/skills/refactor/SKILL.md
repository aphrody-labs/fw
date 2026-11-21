---
name: refactor
description: "Audit a whole codebase, its tests or its docs for defects, waste and drift, then ship the findings as a sequenced PR train."
---
# Deep refactor

`deep-refactor` covers code, `test-refactor` one test suite, `doc-refactor` every markdown file.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/deep-refactor.md](references/deep-refactor.md) | Audit an entire codebase for defects, waste and debt. Triggers: deep refactor, refactor audit, clean the codebase, DRY pass, audit and fix everything. |
| [references/test-refactor.md](references/test-refactor.md) | Audit one test suite at a time for dead weight, duplication, parallel-unsafety and runtime cost. Triggers: test refactor, characterize before refactor, characterization tests, clean the tests |
| [references/doc-refactor.md](references/doc-refactor.md) | Audit every markdown file in a repo — READMEs, docs/. Triggers: doc refactor, refactor the docs, clean the docs, documentation audit, README audit, docs are stale |

<!-- merge-skill-families:v1 -->
