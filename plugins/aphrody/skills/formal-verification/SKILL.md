---
name: formal-verification
description: "Formal specification and verification of real code with AI agents: Lean 4 specs and proofs. Use before planning a verification campaign or designing an agent done-gate."
---
> machine-code semantics (AArch64), crypto protocol security proofs, spec-writer / prover / gatekeeper agent pipelines, and the acceptance-gate lessons for our own multi-agent work.


# Formal verification with agent teams (AlphaProof Code method)

Source: Google DeepMind and Google, "AlphaProof Code: Verifying Signal's Security at the Machine-Code Level" (2026-11-10, 57 pages). Lean development: https://google-deepmind.github.io/code-verification (read 2026-11-21). Numbers below are the PDF's; the live web edition has drifted, see `references/paper.md`.

## Core idea

Proofs are cheap once the spec is fixed; the spec is the hard, human, trusted part. Agents write proofs at scale (about 6.5M Lean lines, none by humans) and are never trusted: a Lean kernel plus the `comparator` tool accepts a proof only if it proves exactly the human-approved statement.

## Method, in order

1. Choose scope and reference models (papers, standards) by hand; pick a small, stable, high-value core. Lean fits crypto, parsers, allocators, protocol state machines, not whole products.
2. Build the target as an artifact: compile the unmodified code with the stock compiler and load the bytes into Lean together with a machine-code semantics (here AArch64, 467 instructions, 31 syscalls). Use a thin CLI harness so the spec speaks about process inputs and outputs.
3. Write the spec with a spec-writer agent under close human direction: Lean theorem statements plus every definition they use, extern contracts ("hooks") for primitives you do not verify. A reviewer agent of another model family attacks it for faithfulness and strength. Human approves; the commit is the baseline.
4. Test the spec: prove non-vacuity (hypotheses satisfiable, honest runs meet premises), inject mutants and reintroduce past bugs and prove the theorem then fails.
5. Launch the prover team (orchestrator, team leads, workers, architect, integrator, support, critic). Rules: orchestrator never edits; one writer per file; only the integrator commits; falsify goals first; shared bulletin file; every new invariant carries a reachability witness.
6. Handle spec errors by amendment only: necessary (machine evidence), non-weakening, small, critic-reviewed without seeing the proof attempts, human-approved when it changes trusted assumptions, logged. Weakening to match the code is a potential bug, escalate it.
7. Gate acceptance: held-out copy of the spec no agent can write; deterministic `comparator` run; statement and all transitive definitions identical; only standard axioms; kernel replay in a fresh environment; `sorry` and `native_decide` rejected; only the gatekeeper declares done; rerun by hand at the end.
8. Report the trusted base (about 18k spec lines plus 8k semantics lines against 6.5M proof lines), the assumptions, what is not covered (see limits).

## Router: read what the task needs

| Need | File |
|---|---|
| Citations, headline numbers, section and page map, link check | `references/paper.md` |
| Roles, AArch64 semantics, gatekeeper and acceptance | `references/architecture.md` |
| Writing and testing specs, trusted computing base | `references/specification.md` |
| Prover strategies, decomposition, failures, scale | `references/proving.md` |
| What was and was not proven, threats, future work | `references/results-and-limits.md` |
| Rules for our own workers (gates, ratios, amendments) | `references/lessons-for-agents.md` |
| Compact digest for agents | `llms.txt` |

## Use for our work

- Verification campaign on one of our cores: start at steps 1-4, do not start provers before a human-approved spec exists.
- No Lean involved: apply `lessons-for-agents.md` rules 1-12 and 17-20 to worker pipelines (independent gate job on the build host, held-out acceptance set, amendment path, mutation and past-bug regression, trusted-base ratio).
- Policy fit: gates are done-gates computing an `accepted` record for a SHA; they do not pause workers or add deny lists.

## Limits to state in any report

Spec errors are caught by review and testing, not guaranteed; hooked contracts and the machine-code semantics are trusted, not proved; the paper reports no token or dollar cost, only single-machine, 24 to 72 hour campaigns of 40 to 80 agents and several-hour clean builds; its theorems cover one build of one CLI, one-to-one sessions, classical computational adversaries.
