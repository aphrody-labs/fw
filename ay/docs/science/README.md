# Science base

Condensed, cited foundations for optimising Bun and Rust, dated 2026-11-17. Every claim carries a source in the References of its page; every computed
number shows its inputs and is reproducible with `bun tools/science.ts`, `bun tools/serve-closed-loop.ts` or the programs in `examples/`.
Agent skills that apply these pages (in the claude config repository, `skills/`): `perf-math`, `async-design`, `recursion-design`, `rust-upstream`, `bun-upstream`.

| Page | Content |
|---|---|
| [async.md](async.md) | Structured concurrency, futures as state machines (measured sizes), readiness versus completion I/O, work stealing, backpressure, Little, Amdahl, Gustafson, USL, M/M/1/K, M/M/c, M/G/1, worked Bun.serve and tokio calculations |
| [recursion.md](recursion.md) | Stack depth bounds (measured), tail calls and `become`, CPS, memoisation, master theorem, recursion schemes, termination by well-founded induction |
| [compilers-languages.md](compilers-languages.md) | Rust blockers (parallel front end, next-gen solver, Polonius, async drop, AFIT/RTN, const generics, specialization, tail calls, linkers, backends) and Bun/JSC/Zig issue classes, read live |
| [os.md](os.md) | io_uring security, sched_ext, Rust for Linux, Windows NT I/O, kqueue |
| [limits.md](limits.md) | Halting, Rice, Curry-Howard, data-race-freedom, P-completeness, FLP, CAP, quorum bounds, NP-hard compiler problems |

Method: facts about moving systems (versions, issue states, counts) are read with `gh api`, kernel.org, learn.microsoft.com or the project's own site on the stated date and marked
**unverified** when a primary source was not reachable; theorems cite the original paper; measurements name machine and tool versions (i7-13700F, Windows 11, Bun 1.4.5, rustc 1.102.0-nightly 2026-11-11)
and are single runs on a shared desktop unless stated (spread about 25%). Re-read a row before acting on it: statuses change weekly.
