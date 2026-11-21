# Proving: prover-agent strategy and scale

Source: sections 2.2-2.3 (p4-6), 4.1 (p12), 7 (p25-27), Appendix C.3-C.4 (p47-51). Own words.

## Division of labour

No human wrote proof code. Humans chose scope and order, reference models, the structure of the specs, the overall proof strategy (costed reduction language; event layer plus concretization plus refinement up to collisions), adjudicated escalations, and designed the agent skills (section 7, p27).

## Team structure and context management

- Hierarchy: orchestrator, team leads, workers; leads shield the orchestrator from back-and-forth and report short summaries (open obligations, completed lemmas, blockers). Reason: limited context windows and state lost at compaction (section 2.3).
- Fixed roles keep the shared codebase coherent: architect (decomposition), integrator (merge, dedupe, generalize, sole committer), support agents (build scheduling, monitoring), per-file single-writer rule.
- Typical campaign: 40 to 80 agents, one machine, 24 to 72 hours. Project: 5 humans, 4 weeks, each running agents on own workstation across symbolic and computational campaigns.
- A monitoring agent above the orchestrator is the only interface for humans; early on monitoring consumed a lot of human time, later less as the skills matured. The authors expect such teams to improve themselves.

## Strategy rules that emerged (section 7, p26)

- Shared bulletin file for notices instead of repeated messaging.
- Leads try to refute a goal before assigning it (falsify first), fix shared parameters across callees, and point workers to existing lemmas.
- Support agents keep the campaign healthy: they group Lean builds within memory limits, spot stuck agents or spent quotas, and speed up slow modules.
- Team composition is not fixed; the orchestrator changes roles and head count over the campaign.
- Every new invariant needs a witness that it holds in some reachable state: blocks progress by proving vacuous lemmas.
- Integrator removes duplicate lemmas and generalizes for reuse, so lemma libraries grow as the campaign proceeds.

## Technique at the binary level

Compositional separation logic for AArch64: one contract per compiled function (including inlined library code, protobuf, session serialization, the SPQR state machine, musl startup and allocator), proved by symbolic execution; theorem holds for every code placement matching the ELF bytes, so addresses and relocations are checked against bytes, not trusted from the linker. A separate theorem bounds heap use through the verified allocator (a session record stays below about 1.2 MB) so no run reaches out-of-memory, which removes OOM terms from the security statements (section 4.1).

The security layers sit on top of one shared set of binary theorems ("the executable does what the Lean model prescribes, including error cases"): 5.3M of the proof lines are these shared binary proofs (section 7).

## Failure handling

- Wrong specs: proof attempts that expose spec errors go through the amendment path; weakening is escalated as a possible code bug (see architecture.md).
- Weakening pressure: agents under pressure to finish may add a hypothesis, narrow a precondition, or redefine a dependency; never trust an agent's "done" report; the gatekeeper sees only paths and names and replays everything in a fresh environment; `sorry` and `native_decide` are rejected.
- Unprovable subgoals: falsify first; counterexamples on the binary or machine-checked disproofs count as evidence for amendments.
- Resource failures: stalled agents and exhausted quotas are watched by support agents; builds batched to fit memory.
- Rebuild churn: a clean proof build could take several hours; small source edits or a recompiled binary early on forced large proof repairs; better program-semantics infrastructure later reduced it. A new libsignal-cli build needs a new refinement proof but not a new security proof (Appendix B.8, p45).
- Large search over proofs is accepted as expensive: the paper sees substantial room to optimize AI-generated proofs and techniques.

## Scale numbers (PDF edition)

| Item | Number |
|---|---|
| Lean repo | nearly 7M lines |
| Proofs | about 6.5M: 5.3M binary, about 860k computational, about 350k symbolic |
| Reintroduced-bug disproofs | 140k |
| Allocator verification | 70k |
| Verified code | about 560 KB, about 140,000 instructions |
| Agents per campaign | 40 to 80 |
| Campaign wall time | 24 to 72 hours on one machine |
| Humans | 5, over 4 weeks |
| Proof lines written by humans | 0 |
| Proof : trusted ratio | about 250 : 1 |

No token, dollar or GPU-hour figures are reported. Cost anchors given for contrast: seL4 about 20 person-years, CompCert about 6 (section 1). Live web edition numbers differ; see paper.md.
