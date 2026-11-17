# Theorems that bound what is possible

Each result states what it forbids, so an agent can recognise an impossible request ("always find every bug", "make consensus
never block") and choose the weaker, achievable goal. Sources in the References; nothing here depends on a live service.

## 1. Computability

**Halting (Turing 1936 [1]).** No total computable $h$ decides, for every program $p$ and input $x$, whether $p(x)$ halts.
Proof sketch: given such $h$, define $d(p)$: if $h(p,p)$ then loop forever, else return. Then $d(d)$ halts iff $h(d,d)$ says it does not:
contradiction.

**Rice (1953 [2]).** For every non-trivial semantic property $P$ of partial computable functions (neither all nor none of them has it), the
set $\{e \mid \varphi_e \in P\}$ is undecidable. Proof by reduction: if $P$ were decidable, pick $e_0$ with $\varphi_{e_0}\notin P$ (or the dual) and
$e_1$ with $\varphi_{e_1}\in P$; from $(p,x)$ build $q$ that runs $p(x)$ then behaves as $e_1$; $q\in P$ iff $p(x)$ halts, deciding halting.

Consequences for compilers and tools:
- No exact static analysis of dead code, aliasing, bounds, termination, or "this optimisation preserves behaviour" for all programs. Every
  analysis is sound-but-incomplete (rejects some good programs) or complete-but-unsound.
- The Rust borrow checker is the first kind: it rejects programs that are safe but not provably so (NLL problem case #3, which Polonius
  targets, `compilers-languages.md`); a more precise checker moves the line but cannot remove it.
- Compiler optimisation has no ceiling: for any compiler there is a program a smarter compiler optimises better ("full-employment theorem"
  for compiler writers, a corollary of Rice). Time spent on a new pass is justified by benchmarks, not by completeness.
- Termination of recursion, trait solving and macro expansion is bounded by limits (`recursion.md` section 7), not decided.

## 2. Logic and types

**Curry-Howard correspondence (Howard 1980 [3]).** Types are propositions, programs are proofs, evaluation is proof normalisation:
$A \to B$ is implication, $A \times B$ conjunction, $A + B$ disjunction, the empty type falsity. Consequences:
- A language with unrestricted recursion has a term of every type, $\mathrm{fix} : (A \to A) \to A$, so as a logic it is inconsistent; proof assistants
  restrict recursion to terminating definitions (`recursion.md` section 7). In Rust, `!` (the never type) is falsity, and `loop {}` inhabits it.
- Substructural logics match resource types: linear logic (Girard 1987 [4]) corresponds to linear types, and Rust's ownership is an affine discipline
  (use at most once); soundness of Rust's core with unsafe libraries is proved semantically in RustBelt (Jung et al. 2018 [5]).
- The proofs-as-programs view justifies property tests and refinement types as partial proofs; it does not remove Rice: type checking a
  sufficiently expressive system (C++ templates [6], dependent types) is itself undecidable or must be fuel-bounded.

## 3. Concurrency and memory

**Data-race-free implies sequential consistency (Adve and Hill 1990 [7]; C++11/Rust model: Boehm and Adve 2008 [8]).** A program with no
data races on atomics-respecting accesses behaves as if threads were interleaved in a single order. Data races are undefined behaviour in
C++ and in `unsafe` Rust (safe Rust excludes them by construction); the optimiser is allowed to assume their absence.

**Parallel speedup.** $S(N) \le 1/\big((1-p) + p/N\big)$ (Amdahl [9]) and, from the work-span law, $T_P \ge \max(T_1/P, T_\infty)$ (`async.md`).
A P-complete problem is inherently sequential: unless $\mathrm{P} = \mathrm{NC}$, no polylog-time parallel
algorithm exists for it with polynomially many processors (Greenlaw, Hoover, Ruzzo [10]); circuit evaluation is the canonical example.

## 4. Distributed systems

**FLP (Fischer, Lynch, Paterson 1985 [11]).** In an asynchronous system (no bound on message delay or relative speed), no deterministic protocol solves
consensus and is guaranteed to terminate if even one process may crash. Practical systems (Paxos, Raft) keep safety always and obtain
liveness from timeouts, leader election or randomisation, i.e. from partial synchrony (Dwork, Lynch, Stockmeyer 1988 [12]).

**CAP (Brewer 2000; Gilbert and Lynch 2002 [13]).** A networked read/write register cannot be at once linearizable (consistent), always available to
every non-failed node, and tolerant of network partitions; during a partition choose consistency or availability. PACELC adds that even without
a partition there is a latency versus consistency trade-off (Abadi 2012 [14]).

**Quorums and fault bounds.** Crash faults: $n \ge 2f + 1$ nodes tolerate $f$ failures (majority quorum $q = \lfloor n/2 \rfloor + 1$; two quorums
intersect because $2q > n$). Worked: $n = 5$, $f = 2$, $q = 3$, $2q = 6 > 5$. Byzantine faults: $n \ge 3f + 1$ (Lamport, Shostak, Pease 1982 [15]):
$f = 1$ needs $n = 4$. **Two Generals:** no finite message exchange over a lossy channel yields common knowledge, hence the idempotency keys and
exactly-once-by-deduplication patterns of production services.

## 5. Optimisation limits that are known to be hard

- Register allocation by graph colouring is NP-complete in general (Chaitin 1982 [16]); on SSA form the interference graph is chordal and colouring is
  polynomial (Hack, Grund, Goos 2006 [17]), which is why modern backends allocate on SSA.
- Optimal instruction scheduling and optimal code layout are NP-hard; compilers use heuristics, so measured speed (criterion, hyperfine) is the
  arbiter, never the argument that a transformation "must" help.
- Phase ordering has no optimal static order; the same pass list can lose on a given program. PGO replaces guesses by measured profiles.

## What an agent does with this

| Request | Theorem | Achievable replacement |
|---|---|---|
| "Prove the program never loops" | Halting, Rice | prove termination of a measure, add fuel or timeout |
| "Lint finds every bug" | Rice | sound subset, tests, fuzzing, proofs of chosen properties |
| "Consensus that never blocks and never errs" | FLP | partial synchrony with timeouts, safety first |
| "Strong consistency and 100% availability on a partitioned network" | CAP | choose per operation; say which |
| "8x more cores, 8x faster" | Amdahl, work-span | measure $p$ or $T_\infty$ first |
| "Optimal code from the compiler" | Rice, NP-hardness | benchmarks and PGO |

## References
1. Turing, "On computable numbers, with an application to the Entscheidungsproblem", 1936: https://doi.org/10.1112/plms/s2-42.1.230
2. Rice, "Classes of recursively enumerable sets and their decision problems", 1953: https://doi.org/10.1090/S0002-9947-1953-0053041-6
3. Howard, "The formulae-as-types notion of construction", 1980 (written 1969): https://www.cs.cmu.edu/~crary/819-f09/Howard80.pdf
4. Girard, "Linear logic", Theoretical Computer Science 1987: https://doi.org/10.1016/0304-3975(87)90045-4
5. Jung, Jourdan, Krebbers, Dreyer, "RustBelt: securing the foundations of the Rust programming language", POPL 2018: https://doi.org/10.1145/3158154
6. Veldhuizen, "C++ templates are Turing complete", 2003: https://rtraba.files.wordpress.com/2015/05/cppturing.pdf
7. Adve and Hill, "Weak ordering: a new definition", ISCA 1990: https://doi.org/10.1145/325164.325100
8. Boehm and Adve, "Foundations of the C++ concurrency memory model", PLDI 2008: https://doi.org/10.1145/1375581.1375591
9. Amdahl, AFIPS 1967: https://doi.org/10.1145/1465482.1465560
10. Greenlaw, Hoover, Ruzzo, "Limits to parallel computation: P-completeness theory", Oxford University Press 1995.
11. Fischer, Lynch, Paterson, "Impossibility of distributed consensus with one faulty process", JACM 1985: https://doi.org/10.1145/3149.214121
12. Dwork, Lynch, Stockmeyer, "Consensus in the presence of partial synchrony", JACM 1988: https://doi.org/10.1145/42282.42283
13. Gilbert and Lynch, "Brewer's conjecture and the feasibility of consistent, available, partition-tolerant web services", SIGACT News 2002: https://doi.org/10.1145/564585.564601
14. Abadi, "Consistency tradeoffs in modern distributed database system design", IEEE Computer 2012: https://doi.org/10.1109/MC.2012.33
15. Lamport, Shostak, Pease, "The Byzantine generals problem", TOPLAS 1982: https://doi.org/10.1145/357172.357176
16. Chaitin, "Register allocation and spilling via graph coloring", 1982 (SIGPLAN Symposium on Compiler Construction): https://doi.org/10.1145/800230.806984
17. Hack, Grund, Goos, "Register allocation for programs in SSA-form", CC 2006: https://doi.org/10.1007/11688839_20
