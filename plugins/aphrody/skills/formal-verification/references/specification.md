# Specifications: writing, review, approval, trusted base

Source: sections 2.1 (p3-4), 4 (p9-12), 5.1-5.3 (p13-24), 6 (p24-25), 7 (p25-27), Appendices B-C. Own words.

## What a spec is

A set of Lean theorem statements plus every definition they depend on: models of the environment, the attacker, the process interface, and extern contracts for parts outside scope. It defines what correct or secure means and which bug and attack classes the proof excludes. It cannot be machine-checked, so it is the trusted artifact; the proofs are not (section 2.1).

## Principle (section 2, p3; section 7, p25)

Once proof effort collapses, nearly all human effort moves to engineering the specification: what to prove and which assumptions the machine code runs under. Most project effort went there. The paper's discussion (section 9, p30) calls a nontrivial spec a nontrivial software artifact that needs its own engineering discipline.

## How specs are written

1. Start from the best published model: Signal symbolic models (Bhargavan et al. PQXDH; Cheval-Jacomme-Richards Double Ratchet) via the DyLean Lean framework; computational game from Fiedler-Guenther (handshake) plus Alwen-Coretti-Dodis (ratchet).
2. Compose and extend where literature stops: prior work analyzed handshake and ratchet separately and predates the post-quantum ratchet; composing took several draft-and-review rounds.
3. The writer drafts a small set of top-level statements per component (binary contracts of an entry point, the main computational theorem, ...).
4. A reviewer agent from a different model family checks faithfulness to the source paper and to libsignal, strength (would it rule out known or plausible bugs), and consistency with literature conventions where the spec goes beyond it.
5. Many rounds of targeted human feedback; then the user approves and pins the commit as the proof target.
6. Legibility is a requirement: left alone, agents wrote specs that were hard to audit, so humans directed rewrites; this paid off when late reviewers joined.
7. Keep a deviation catalog: every place the Lean statement departs from the paper or from libsignal, with its effect. About 200 entries (computational) and about 35 (symbolic). Real findings: 15 gaps in a published Double Ratchet analysis (missing hypotheses, trivial-win conditions); symbolic queries in a published paper that were false as printed (wrong ratchet index for skipped keys, an unbound variable, a mis-bracketed post-compromise clause); an unprovable literal reading of a PQXDH corollary (section 7, p26).

## How spec bugs are avoided

Failure modes named in 2.1: over-restrictive assumptions on inputs, weak conclusions, too few constraints on existential witnesses; a weakened attacker model weakens the whole guarantee. Controls:

- Human review, accepted as insufficient alone.
- Skeptical reviewer agents (effective at soundness and strength problems).
- Non-vacuity theorems proved in Lean: hypotheses jointly satisfiable, premises met by concrete honest runs (e.g. sessions with and without one-time prekey reachable; the forward-secrecy and handshake-secrecy premises satisfiable; concrete example runs satisfy Theorem 1 with no collision; the cryptographic premise set shown jointly satisfiable on a real prime-order group, Appendix C.8).
- Mutation testing: agents inject natural bugs (e.g. skip ciphertext MAC verification) and prove the security theorem false on the mutated binary.
- Historical bugs: two fixed libsignal bugs (weak-state, commit cd36118; small-subgroup, commit 5293caa) were reintroduced by reverse-applying the fix; Lean proves the symbolic theorem fails on each binary, using witness inputs and kernel stepping of the SHA-256-pinned ELF (section 6). Refuting the computational theorem this way would require proving hardness assumptions, so it is not done.
- Specification shape rules found in the appendices: state exceptions as events on the trace rather than "attacker knows X", so a weaker attacker cannot break a query (B.8, p45); represent reductions in a small deeply embedded language with a cost semantics so "there exists a reduction" cannot be satisfied by an uncomputable or exhaustive-search function (5.2.2, p20-21); map model terms to bytes with a one-way concretization and prove refinement up to collisions, since a guessed 8-byte MAC can legitimately be accepted (5.1.4, p15-17); fix the coin-freshness assumption openly.
- Amendment process (see architecture.md) for specs that are wrong about code, with a rule that security-weakening changes escalate to a human as a possible bug.

## Trusted computing base (section 5.3, p24)

Reader must check theorem statements and, transitively, every definition used.

| Layer | Trusted items |
|---|---|
| Symbolic proof | DyLean model and security statements; the thin event layer mapping runs of the binary to protocol events; concretization function; refinement statement with collision clause; environment restrictions |
| Computational proof | game definitions, assumption games, the bound, the Lean model of libsignal and SPQR, the random-oracle step |
| Both | AArch64 semantics and Linux process model; contracts of hooked routines; functional and algebraic premises on primitives |
| Tooling | extraction of symbol names from object files; reproducible build scripts |
| Outside the TCB | Rust compiler, Rust source semantics, all proofs, program logic, refinement and coupling arguments, reductions, function contracts |
| Always | Lean kernel (small audited kernel; independent kernels and consistency proofs exist) |

Size: about 18,000 lines of statements and definitions plus about 8,000 lines of semantics (section 7, p25). Each spec file is labeled trusted, trusted in part, or Lean-checked. A bug in Rust, the compiler or an untrusted component can only make the theorem false, never silently accepted.

## Scope the spec states openly

Process model: fresh non-setuid process, fixed load address, empty environment, only stdin/stdout/stderr open, fixed clock, kernel randomness fixed so all entropy comes from a coins file, 1 GiB memory and 8 MiB stack (section 4.1; Appendix C.5, p52). Exit code 0 means accepted and writes files; 1 means rejected and writes none; 2 means bad command line. Hooked routines are listed by name and count (39 plus 18 panic entries).
