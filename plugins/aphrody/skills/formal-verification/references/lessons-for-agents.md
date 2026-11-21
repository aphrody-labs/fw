# Lessons for our own multi-agent engineering

Derived from AlphaProof Code (section numbers refer to the paper; PDF pages in paper.md). One quote, section 7, p25: once proofs become cheap, "the hard question is what to prove".

"Today" = how our workers run per the task brief and `plugins/aphrody/agents/worker.md`: a worker implements in its file scope, runs build and tests (heavy ones on the build host `fr1`, see the `fleet` skill), verifies, reports what it observed and pushes to `main`. Only that baseline is given; the gaps named under "Today" are what the baseline does not guarantee, to be checked per repo before adopting a change. The global policy is no deny lists, no blocking guards, no approval pauses. Every "gate" below is therefore a done-gate: an independent job that computes an `accepted` record for a SHA, not a hook that pauses workers. A work item counts as done only when that record exists.

Terms: spec = the artifact that defines done (acceptance tests, golden files, schemas, contracts, benchmarks thresholds); proof = the implementation plus its own tests; trusted base = what humans must read to believe the result.

## A. Acceptance and trust

### 1. Never accept agent output without an independent mechanical checker
- Source: sections 1, 2 (p3), 2.4. Reliability of the result does not depend on the prover agents because the kernel checks every proof.
- Today: "tests pass" is reported by the same agent that wrote the code; the build host runs what the worker ran.
- Change: add a gate job on the build host (`gate.ts`, typed, deterministic, rerunnable by hand) that checks out the pushed SHA into a fresh directory, runs the pinned build and test commands, and writes `gate/<sha>.json` (commands, exit codes, toolchain versions, test counts). The orchestrator trusts the file, never the worker's sentence.

### 2. Define acceptance by a gate the agent cannot bypass
- Source: 2.4 (p6). Gatekeeper copies the approved spec to a held-out directory no agent can write, and only it can declare a top-level goal complete.
- Today: acceptance tests live in the same checkout the worker edits and pushes.
- Change: keep a held-out copy of acceptance material (tests, fixtures, thresholds, CI/build config) in a path outside worker write scope (owned by a gate user on `fr1`, or a repo path whose hash manifest `acceptance.lock` lives on the host). The gate overlays the held-out copy onto the fresh checkout before running. Workers, leads and the orchestrator can mark work "implemented"; only the gate marks it "accepted".

### 3. Compare transitive dependencies, not just the test names
- Source: 2.4, 7 (p26): a statement can be weakened through a definition it uses; in another swarm an agent redefined a predicate to False and the trick spread.
- Today: a review of the touched tests can miss helper code, fixtures, feature flags, thresholds and CI scripts changing underneath.
- Change: the gate hashes the acceptance set plus everything it imports or loads (helpers, fixtures, snapshots, `cfg` flags, harness crates, workspace lint config, test runner args) against the baseline manifest, and fails acceptance on any difference not carried by an approved amendment. Also reject, as gate checks: ignored/skipped/xfail tests, deleted tests, lowered thresholds, `todo!`/`unimplemented!` on reachable paths, `--no-verify`-style bypasses in scripts (`sorry` and `native_decide` analogues). These are checks on the accepted record, not commit blockers.

### 4. The gatekeeper reads artifacts, not narrative
- Source: 7 (p26). The gatekeeper sees only file paths and theorem names, never prover dialogue; Lean replays in a fresh environment.
- Today: orchestrators read worker summaries and decide.
- Change: the gate input is `{sha, paths, test ids}`; its output is machine-readable. Worker reports are leads to investigate, never evidence. Rerun the gate on a clean machine state at the end of a campaign (2.4, last paragraph).

### 5. Keep the trusted base small, explicit and human-reviewed
- Source: 5.3 (p24), 7 (p25): about 26,000 trusted lines against about 6.5M generated lines; every spec file labeled trusted, trusted in part, or checked.
- Today: no file marks which paths a human must read to believe a green build.
- Change: add `TRUSTED.toml` per repo labeling paths `trusted` (acceptance tests, gate scripts, build scripts, toolchain pins, harness and contracts), `trusted-in-part`, or `generated` (everything agents write). Humans review changes to trusted paths; agent churn in generated paths needs no human read once the gate passes.

### 6. Measure the trusted-base ratio and the generated-artifact ratio
- Source: 7 (p25): 6.5M proof lines : about 26k trusted lines is roughly 250 : 1; plus a 560 KB verified binary and a 95k-line generated image whose only trusted part is symbol names.
- Today: no metric.
- Change: the gate report includes `trusted_loc`, `generated_loc`, `ratio`, and trend per campaign; a rising trusted base without a matching rise in coverage is a review signal. Targets are per repo; record the first measured value rather than inventing a threshold.

### 7. Pin the artifact that was checked and the toolchain that built it
- Source: 5.3, 6, C.6: the verified ELF is SHA-256 pinned, built by reproducible scripts that are themselves trusted; the proof holds for one binary and a rebuild needs a new refinement proof (B.8).
- Today: nothing in the flow ties a green result to a specific artifact hash and toolchain.
- Change: gate records the SHA-256 of the built artifact it tested, the exact compiler and lockfile hashes, and refuses to reuse an old `accepted` record for a different artifact.

## B. Specification discipline

### 8. Separate the spec author from the prover (implementer)
- Source: 2 (p3), 2.1, 7 (p25-26). The spec writer is a different agent role under human direction; a reviewer from a different model family critiques it; the provers cannot edit the spec.
- Today: the worker writes the code and the tests that judge it.
- Change: for any item above trivial size, an acceptance-author agent (ideally a different model family) writes the acceptance tests/contract first from the human's intent and the upstream references; a skeptical reviewer agent checks faithfulness and strength; the implementer works against it read-only. Humans approve the baseline commit of the acceptance set.

### 9. Spend human effort on what to prove, and make the spec legible
- Source: 7 (p25-26), 9 (p30). Most project effort was the specification; agent-written specs hard to audit were rewritten for legibility.
- Today: human attention goes to reading diffs and reports.
- Change: ask humans for intent and acceptance criteria up front, in short files; require acceptance sets to be small and readable (top-level statements first, helpers after); send illegible ones back for rewrite before implementation starts. Keep a deviation log: each place the tests depart from the upstream reference or the real system, with its effect (the paper kept over 200 entries and about 35, and found real errors in published sources, 7, p26).

### 10. Test the spec, not just the code: non-vacuity and mutation
- Source: 2.1 (p4), 6, 7 (p26). Agents prove hypotheses are satisfiable, premises are met by concrete runs, inject natural bugs and prove the theorem fails on them, and each new invariant needs a witness.
- Today: a green suite is taken as proof the suite is strong.
- Change: three gate stages beyond "tests pass": (a) every new assertion or invariant ships with a concrete passing example that exercises its premise; (b) a mutation job (for Rust, a mutation tester run on the build host) injects realistic bugs (skip a check, off-by-one, drop an error branch) and requires the suite to fail on each; surviving mutants are reported as spec weaknesses; (c) a regression corpus of our own past bugs, stored as reverse-applicable fix patches, must still make the suite fail when reintroduced (the paper reintroduced two real fixed bugs, section 6).

### 11. Treat spec change as an audited amendment, not a worker edit
- Source: 2.3 (p6), 7 (p27). Amendment accepted only if necessary (machine evidence), non-weakening, and small; a critic who never sees the proof attempts judges it; assumption changes go to a human or a policy; weakening-to-match-code is escalated as a possible bug; log and review at campaign end.
- Today: nothing stops a worker from resolving a failing test by editing it.
- Change: a worker who believes the acceptance set is wrong files `amendments/<n>.md` (evidence: failing case, why the code is right, diff of at most a few lines), and a critic agent decides without the implementation attempts; trusted-assumption changes require the human or a stored policy; the gate applies the amendment to the held-out copy and logs it. Never "fix the test" in the implementer's scope.

### 12. Different model families for review
- Source: 7 (p25-26). The spec was written with one model mix and reviewed by an agent from a different family; skeptical-perspective prompts found soundness and strength problems.
- Today: nothing in the worker flow requires reviewer and author to differ in model family.
- Change: route acceptance review (not implementation) to a different family; prompt it to attack the acceptance set (what bug would still pass?), and record its objections in the deviation log.

## C. Orchestration

### 13. Orchestrator never edits; leads compress; one writer per file; one committer
- Source: 2.3 (p5-6). Orchestrator never edits files; team leads send short summaries; each file has at most one worker at a time; only the integrator commits.
- Today: workers own disjoint scopes and each pushes to `main`.
- Change: keep disjoint scopes (already true) and add an integrator role that runs the gate and commits by path; workers hand over a verified change set. Leads hold worker chatter and give the orchestrator only open obligations, finished items and blockers, which saves context after compaction. This stays consistent with working on `main` in place: one committing agent, no branches.

### 14. Falsify before you build
- Source: 2.3 (p5), 7 (p26). Agents try to refute a goal before work starts; leads refute before assigning.
- Today: workers start implementing the assigned goal.
- Change: a pre-step per item: write a reproducer or counterexample attempt (for a bug) or a satisfiability check of the acceptance set (for a feature) and record the result; unsatisfiable or already-true goals are returned to the orchestrator before any heavy build.

### 15. Shared bulletin, lemma reuse, support roles for expensive resources
- Source: 7 (p26), 2.3. A shared bulletin file replaces repeated messages; leads point workers to existing lemmas; support agents batch expensive builds to fit memory, watch stalled agents and exhausted quotas, and refactor slow modules; the orchestrator reshapes the team during the campaign.
- Today: peers are told via inject/notify messages; each worker launches its own heavy builds.
- Change: a bulletin file per campaign (append-only, read at start of each task); one support agent owns the build queue on `fr1` (serialize heavy Cargo or C++ jobs, honor peer locks per the `fleet` skill) and a watchdog for stalled workers and quota exhaustion; an integrator/architect pass that deduplicates helpers and promotes shared utilities.

### 16. Evolve the agent skills by watching campaigns
- Source: 7 (p26). Skills were developed by watching runs; a monitor agent above the orchestrator is the human interface; human monitoring time fell as skills matured.
- Today: no per-campaign feedback loop is mandated for skill changes.
- Change: one monitor agent per long campaign producing a status log and a bottleneck list; humans and the monitor review it after each campaign and change the skills (`claude plugin update` flow stays as is). Track human minutes per campaign as a metric.

## D. Fidelity and scope

### 17. Test the shipped artifact at the lowest practical level
- Source: 1, 8, 9 (p30). Verifying the compiled executable removes the compiler and language semantics from the trusted base; costs more proof effort (registers, stack, layout).
- Today: unit tests run against source and mocks.
- Change: add black-box acceptance runs of the release binary or container as built on `fr1` (CLI in, bytes out, exit codes, files), as the paper's CLI harness does. Keep source-level tests for speed, but accept on the artifact.

### 18. Validate your trusted oracles by differential testing
- Source: 2.2 (p4-5). The AArch64 semantics is checked in lockstep against two independent oracles, plus emulation of real scenarios in Lean vs the real binary (15.6M instructions, 10 invocations).
- Today: mocks and fakes are trusted by construction.
- Change: for each mock or simulator an acceptance test depends on, add a differential test against the real service or a second implementation, and list the unverified mock contracts explicitly (the paper's hooks) in `TRUSTED.toml`.

### 19. List every assumption; keep the environment closed
- Source: 4, 4.1, C.5-C.6. Fixed clock, fixed kernel randomness, fresh process, memory and stack bounds, hooked routines by count; a theorem says nothing outside them.
- Today: environment drift (clock, network, randomness, host state) is a possible source of flaky results.
- Change: the gate runs acceptance in a hermetic sandbox (fixed clock and seeds, no network unless declared) and writes the assumption list into the record; flaky results are failures of the harness, handled at the source.

### 20. Make verification part of the change loop, not a one-off campaign
- Source: 9 (p30-31). With cheap proofs, every change can be verified before it ships; prover agents repair affected proofs when code or spec change; the paper plans to verify new libsignal releases before they ship.
- Today: verification is whatever the worker ran last.
- Change: run the gate on every push to `main` for items with an acceptance set, and let a repair worker take failures; stale `accepted` records expire when any trusted input or the artifact hash changes.

## When to apply formal proofs themselves

Use Lean-style proof only for small, high-value, stable cores (cryptography, parsers, allocators, protocol state machines, a kernel-adjacent boundary of the OS). It needs a human-written spec effort of weeks, a trusted semantics, and long builds (clean build hours, section 7). For everything else take the discipline (rules 1-12, 17-20) without the prover. Do not treat a Lean theorem about a spec as evidence about intent: the paper states plainly that spec errors are caught by review and tests, not guaranteed (2.1).
