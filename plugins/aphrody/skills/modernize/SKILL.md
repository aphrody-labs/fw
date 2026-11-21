---
name: modernize
description: "Modernize legacy codebases: preflight, assess, map, extract rules, brief, then reimagine, transform or uplift, harden and status. Use for COBOL, legacy Java, C++,.NET and monolith migrations."
argument-hint: "[system] [--source <path>]"
---
# Code modernization

A reference named `modernize-<step>` is the former `/modernize-<step>` command. Workflow scripts are under `references/workflows/` (run with the Workflow tool), the topology viewer under `references/assets/`. Specialist agents are `aphrody:legacy-analyst`, `aphrody:business-rules-extractor`, `aphrody:security-auditor`, `aphrody:architecture-critic`, `aphrody:test-engineer`, `aphrody:scaffolder`, `aphrody:version-delta-analyst`, `aphrody:uplift-migrator`.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/modernize-assess.md](references/modernize-assess.md) | What am I dealing with? Inventory, complexity, debt, security and a recommended modernization pattern |
| [references/modernize-brief.md](references/modernize-brief.md) | Write the phased Modernization Brief — the plan a steering committee approves and the build commands execute against |
| [references/modernize-extract-rules.md](references/modernize-extract-rules.md) | Mine the business rules out of the code into testable Given/When/Then rule cards with file:line citations |
| [references/modernize-harden.md](references/modernize-harden.md) | Security scan of the legacy system with a reviewable remediation patch (OWASP, CWE, CVEs, secrets, injection) |
| [references/modernize-map.md](references/modernize-map.md) | Show me the structure — dependencies, data flow, entry points and business flows, as an interactive map |
| [references/modernize-preflight.md](references/modernize-preflight.md) | Run this first — checks the environment, records where the code lives, and says what to fix before anything else runs |
| [references/modernize-reimagine.md](references/modernize-reimagine.md) | Rebuild the system from its extracted intent on a new architecture, with two human checkpoints |
| [references/modernize-review.md](references/modernize-review.md) | Confirm or correct the business rules that need a person's judgement; the plan and the build read your answers |
| [references/modernize-status.md](references/modernize-status.md) | Where am I? Progress, what is stale, and the exact next command to run — start here when unsure |
| [references/modernize-transform.md](references/modernize-transform.md) | Rewrite one legacy module in the target stack, with tests that prove it behaves the same |
| [references/modernize-uplift.md](references/modernize-uplift.md) | Same-stack version uplift (e.g..NET Framework to modern.NET, Java 8 to the current LTS) — keep the code, fix the version deltas, prove nothing changed |
| [references/modernize-verify.md](references/modernize-verify.md) | Prove the modernized code behaves like the legacy, with an independent re-check and one verdict per module |

<!-- merge-skill-families:v1 -->
