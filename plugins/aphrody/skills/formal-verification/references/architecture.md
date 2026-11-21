# Architecture of AlphaProof Code

Source: paper sections 2.1-2.4 (p3-6) and Figure 1 (p2), 4 (p9-12), Appendix C.3 (p47-50). Own words.

## Four components

1. Spec-writer agent, human-guided: turns models and properties from papers and standards into Lean theorem statements plus every definition they use. Reviewer agents critique each draft; the user approves; the approving git commit is the baseline.
2. Code under verification: the unmodified Rust crates plus a tiny CLI harness, compiled by the stock compiler to an AArch64 ELF and loaded into Lean as a list of bytes.
3. Prover team: orchestrator, team leads, workers, architect, integrator and support agents; all untrusted.
4. Gatekeeper: deterministic script plus an agent; accepts a proof only if the Lean kernel checks it and the Lean `comparator` tool confirms it proves exactly the held-out approved statement.

Every agent, including the spec writer, is a general-purpose coding agent whose behaviour is a "skill": a natural-language document naming the task, the sources to consult and the checks to run (section 2.1, p4). Skills are the tuning surface; the humans iterated them by watching campaigns (section 7, p26).

Trust picture (Figure 1, p2): shaded = a reader must trust it (spec, AArch64 semantics, hook contracts, symbol-name extraction, build scripts); unshaded = agent output with untrusted status. Beyond that, one trusts Lean's kernel and the stated cryptographic assumptions.

## AArch64 machine-code semantics (section 2.2, p4-5)

- Central relation `ElfRunsTo bin pre post`: every run of `bin` from inputs meeting `pre` ends in an outcome meeting `post`, and no run faults (unmapped read, write to executable memory, permission violation).
- Machine state: general and SIMD/FP registers, PC, condition flags, byte memory with per-page permissions, files of the current directory. Decoder from bytes; instruction effects follow Arm's machine-readable ASL spec. ELF loader and initial stack as Linux does; system-call model for memory, file and stream I/O and exit.
- Size: 467 instructions, 31 system calls, 7,532 Lean lines. Single-threaded, statically linked only; no threads, signals, vDSO, dynamic linking, or files outside the current directory.
- Trusted parts are definitions only. The program logic (separation logic rules, weakest-precondition calculus, automation) is not trusted: it is proved sound in Lean against `ElfRunsTo`.
- Validation of trusted definitions by lockstep testing against two independent oracles (Unicorn emulator, Sail model of Armv9.4-A), plus test vectors, random instruction sequences, compiled Rust functions; loader and syscalls checked against qemu-aarch64 on many binaries.
- Two spec-relevant features: physical memory is a finite input (a run can end out-of-memory unless a theorem bounds it); a "hook" replaces a function by a trusted contract (used for crypto primitives, file/stdout writers, some libc and Rust library routines). Hooks are part of the spec and reviewed with care.
- Proof strategy for the binary (Appendix C.3 Lemma 4, p50): every compiled function (musl startup, mallocng allocator, CLI, protobuf, libsignal, SPQR) gets a contract relating registers and memory to a Lean model function; contracts proved by symbolic execution with weakest preconditions, loop invariants, frame rule, bit-vector facts discharged by the kernel; quantified over every code placement matching the ELF bytes.

## Prover-team orchestration (section 2.3, p5-6; section 7, p26)

| Role | Job |
|---|---|
| Orchestrator | splits work, schedules agents, never edits Lean files |
| Team leads | own a larger, longer goal; absorb worker chatter; send the orchestrator short summaries of open obligations, finished lemmas, blockers (context-window defence) |
| Proof workers | one subtask each; at most one worker edits a file at a time |
| Architect | designs and periodically revises the high-level proof decomposition |
| Integrator | merges verified proofs into library modules, dedupes, generalizes lemmas; the only role that commits |
| Support agents | schedule/batch expensive Lean builds to fit memory, watch stalled agents and exhausted model quotas, refactor slow modules |
| Critic | judges spec amendments; never sees the proof attempts |
| Monitor agent above the orchestrator | the human interface; humans read its logs to diagnose bottlenecks |

Working habits: agents try to falsify a goal before starting it; a shared bulletin file replaces repeated messages; leads fix shared parameters (stack bounds) across callees and point workers to existing lemmas; every new invariant must come with a witness that it holds on some reachable state, so vacuous lemmas cannot count as progress.

## Amendments (section 2.3 and 7)

Agents cannot edit the spec. They propose an amendment to the critic, which accepts only when (1) necessary: shown by a machine-checked disproof, a binary counterexample or an argument grounded in code; (2) not weakening security, and any added assumption must hold of the real code; (3) small, typically a few lines in one definition. Amendments touching trusted assumptions need the user or a user-set policy; all are logged with justification on an audited branch and reviewed by humans at campaign end. A proposal that would weaken security to match the code is not an amendment: it goes to a human as a potential bug.

## Gatekeeper and acceptance (section 2.4, p6; section 7, p26-27)

1. At campaign start the gatekeeper copies the spec from the baseline commit into a held-out directory no agent can write.
2. The held-out copy changes only through accepted, critic-reviewed, human-approved amendments applied by the gatekeeper.
3. The gatekeeper calls `comparator` through a deterministic script that can be rerun by hand. File discipline (which files are trusted spec vs untrusted proof) is enforced by scripts.
4. Acceptance requires all of: the theorem statement and every definition it transitively depends on are identical to the held-out copy; only Lean's standard axioms used; the kernel accepts the proof in a fresh environment; `sorry` and `native_decide` rejected.
5. Only the gatekeeper may declare a top-level goal complete; provers, leads and the orchestrator cannot.
6. The gatekeeper sees file paths and theorem names, never the provers' dialogue or explanations.
7. At the end of development comparator is rerun manually for extra assurance.

Why not just compare the theorem text: a statement can be weakened indirectly by editing a definition it uses (an agent in another swarm redefined a hypothesis predicate as False and the trick spread; section 7, p26, refs [78, 107]).
