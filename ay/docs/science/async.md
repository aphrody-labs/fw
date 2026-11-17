# Asynchrony: laws, models and worked calculations

As of 2026-11-17. Every formula cites its source in the References; every number is produced by `bun tools/science.ts`
(tests in `tools/science.test.ts`) or by the commands named beside it. Inputs that are assumptions are marked
**assumed**; inputs that were measured name the machine: i7-13700F (24 threads), Windows 11, Bun 1.4.5, rustc
1.102.0-nightly (2026-11-11).

## 1. Structured concurrency

Invariant (Sustrik 2016 [1], Smith 2018 [2], Elizarov 2018 [3]): a task started in a scope ends before the scope ends.
For every child task $c$ spawned in scope $s$: $\mathrm{end}(c) \le \mathrm{end}(s)$. Consequences: errors propagate to the
parent, cancellation propagates to children, no task outlives its owner's borrowed data.

Languages: Kotlin `coroutineScope`, Swift task groups (SE-0304 [4]), Java `StructuredTaskScope` [5], Trio nurseries [2];
Rust has `std::thread::scope` for threads and, for async, `tokio::task::JoinSet` / `tokio_util` task tracking, but no sound
borrowing scoped async spawn: a future can be leaked (`mem::forget`), so destructors cannot be relied on to join a child
(async drop is the open language work on this, see `compilers-languages.md`).

Rules: spawn only into a scope or a `JoinSet` whose handle you await; every spawned future has a cancellation path
(drop, `AbortHandle`, `AbortSignal`); never detach in library code.

## 2. Futures as state machines

An `async fn` is lowered to a coroutine: an enum with one variant per suspension point, holding exactly the locals live
across that point; `poll(self: Pin<&mut Self>, cx)` advances it to the next `.await` and returns `Ready` or `Pending`;
`cx.waker()` is the edge-triggered notification [6][7]. Size of a future:

$$\mathrm{size}(F) \approx \mathrm{tag} + \sum_{v \in \mathrm{Live}(F)} \mathrm{size}(v)\ \text{(overlapped only when storage is provably dead)}$$

Measured (`rustc --edition 2024 -O`, `size_of::<F>()`, program `docs/science/examples/futures-size.rs`):

| Future | Bytes | Reading |
|---|---|---|
| `async fn none()` (no await) | 1 | tag only |
| one `.await` on an empty future | 2 | tag + inner state |
| `[u8;1024]` held across one await | 1026 | the array lives in the future |
| same array but in an inner block that ends before the await | 2 | storage dead, not stored |
| array 1024 B, then second array 2048 B, both in function scope, two awaits | 3075 | no overlap: 1024 + 2048 + 3 |
| `nested` / `nested2` wrapping the 1 KiB future | 1027 / 1028 | nesting adds 1 byte of tag per level |

Rules: end the scope of large locals before `.await` (the 3075 B case becomes about 2051 B); `Box::pin` large futures
before spawning (tokio moves futures into tasks); a recursive `async fn` needs boxing because the state would be infinite.
JavaScript: `async` functions suspend through promise jobs; a Bun event-loop turn runs one macrotask then drains the
microtask and `process.nextTick` queues (ECMAScript jobs [8]), so a long synchronous loop in a handler stalls every other
connection (see the measurement in section 8).

## 3. Readiness versus completion I/O

| Model | Kernel interface | You are told | Cost per request (recv + send, $o=2$ operations) |
|---|---|---|---|
| Readiness | epoll (Linux), kqueue (BSD, macOS) [9] | the descriptor can be read now | $o + 1/E$ syscalls, $E$ = events per `epoll_wait` |
| Completion | IOCP (Windows) [10], io_uring (Linux) [11] | the operation finished | $o/B$ syscalls, $B$ = submissions per `io_uring_enter`; 0 with SQPOLL |

Worked: $E = B = 16$: epoll $2 + 1/16 = 2.0625$ syscalls per request, io_uring $2/16 = 0.125$; ratio $16.5\times$. This is a
syscall count, not a time: the gain depends on mitigation costs and on kernel-side work, which is why the claim is
verified on the target with `perf stat -e raw_syscalls:sys_enter` before adopting io_uring. io_uring has had a steady
stream of security issues and is disabled in some environments; see `os.md`. Windows I/O ring and registered buffers: `os.md`.

## 4. Work-stealing schedulers

Work-span model: $T_1$ total work, $T_\infty$ critical path, $P$ workers. Any schedule satisfies
$T_P \ge \max(T_1/P,\ T_\infty)$; a greedy scheduler achieves $T_P \le T_1/P + T_\infty$ (Brent); randomized work stealing
achieves $E[T_P] = T_1/P + O(T_\infty)$ (Blumofe and Leiserson 1999 [12]). Parallelism $= T_1/T_\infty$ caps speedup.

Worked, **assumed** $T_1 = 1000$ ms, $T_\infty = 50$ ms: parallelism 20. $P=8$: lower bound $\max(125, 50) = 125$ ms,
greedy upper bound $125 + 50 = 175$ ms. $P=32$: lower bound $\max(31.25, 50) = 50$ ms: 24 extra workers buy nothing.

Tokio's multi-thread scheduler is a work-stealing design: per-worker local run queues (256 slots), a LIFO slot for the
last-woken task, a global injection queue, stealing half of a victim's queue [13]. Consequences: tasks must not block a
worker (a blocked worker removes capacity: $c_{eff} = c - b$); CPU-bound work goes to `spawn_blocking` or rayon; the
LIFO slot favours producer-consumer locality but can starve, hence the cooperative budget (`coop`). Bun's JS runs on a
single thread per event loop: parallelism comes from `Worker`s or multiple processes (`reusePort`), not from stealing.

## 5. Backpressure

A queue without a bound converts overload into latency and then into failure. Reasoning from Little's law
$L = \lambda W$ [14]: with arrival rate $\lambda$ above service rate $\mu$ the backlog grows at $\lambda - \mu$.
Worked, **assumed** $\lambda = 6000$, $\mu = 5000$ req/s: backlog 1000 req/s; after 10 s, 10,000 queued and
$W = 10000/5000 = 2$ s for the newest request, with no bound. Bounded to $K = 128$ (model M/M/1/K, section 7) the same
overload gives blocking probability $P_K \approx (\rho - 1)/\rho = 16.7\%$ and $W \approx 24.6$ ms: the system sheds 1/6 of
arrivals and keeps latency finite.

Mechanisms: bounded channels (`tokio::sync::mpsc::channel(n)`, `Semaphore` permits), demand signalling (Reactive Streams
`request(n)` [15]), TCP receive windows, Bun `ReadableStream` highWaterMark and `Bun.serve` socket `drain` events, and
load shedding at admission (reject with 503 before work starts). Retries multiply load: with failure probability $p$ and $r$
retries the offered load is $\sum_{i=0}^{r} p^i$; $p = 0.5$, $r=3$ gives $1.875\times$; independent retries at $d$ layers
compound to $(r+1)^d$ in the worst case ($4^3 = 64\times$) [16]. Overload that persists after the trigger is gone is a
metastable failure [17]; the cure is shedding plus jittered, budgeted retries, not larger queues. Tail latency under
fan-out of $n$ calls with per-call slow probability $q$: $P(\text{slow}) = 1 - (1-q)^n$; $q = 0.01$, $n=100$ gives $63.4\%$ [18].

## 6. Scaling laws

| Law | Statement | Source |
|---|---|---|
| Little | $L = \lambda W$, any stable system | [14] |
| Response time (closed loop) | $W = C/X - Z$ for $C$ clients, think time $Z$ | [19] |
| Amdahl | $S(N) = 1/\big((1-p) + p/N\big)$, limit $1/(1-p)$ | [20] |
| Gustafson | $S(N) = N - \alpha (N - 1)$, $\alpha$ = serial fraction of the scaled run | [21] |
| Universal Scalability | $C(N) = N / \big(1 + \sigma (N-1) + \kappa N (N-1)\big)$, peak at $N^\* = \sqrt{(1-\sigma)/\kappa}$ | [22] |

Worked (`bun tools/science.ts`):

- Amdahl, $p = 0.95$: $S(8) = 1/(0.05 + 0.11875) = 5.926$; $S(64) = 15.42$; limit $20$. $p = 0.99$: $S(64) = 39.26$, limit $100$.
- Gustafson, $\alpha = 0.05$, $N = 64$: $S = 64 - 0.05 \times 63 = 60.85$. Same serial share at fixed size (Amdahl) gives 15.42: the two laws answer different questions (fixed problem versus fixed time).
- USL, **assumed** $\sigma = 0.05$ (contention), $\kappa = 0.001$ (coherency): $N^\* = \sqrt{0.95/0.001} = 30.82$; $C(8) = 5.690$; $C(30.82) = 9.038$; $C(64) = 7.822$: beyond $N^\*$ adding workers lowers throughput.
- USL fit (`bun tools/science.ts fit <file>`, linear least squares on $N/C - 1 = \sigma (N-1) + \kappa N (N-1)$ [22]): for **synthetic** throughput 1000, 1900, 3480, 5900, 8400, 8900, 5800 at $N = 1,2,4,8,16,32,64$ the fit gives $\sigma = 0.01755$, $\kappa = 0.002208$, $N^\* = 21.09$, $X(N^\*) = 9217$. The test suite proves the fit recovers exact parameters from exact data.

## 7. Queueing models for a server

M/M/1/K (Poisson arrivals, exponential service, one server, at most $K$ in the system) [23]. With $\rho = \lambda/\mu$:

$$P_K = \frac{(1-\rho)\rho^K}{1 - \rho^{K+1}},\qquad \lambda_{eff} = \lambda (1 - P_K),\qquad W = \frac{L}{\lambda_{eff}}.$$

M/M/c (one shared queue, $c$ workers), $a = \lambda/\mu$, $\rho = a/c < 1$: Erlang C
$C(c,a) = \dfrac{\frac{a^c}{c!}\frac{c}{c-a}}{\sum_{k=0}^{c-1}\frac{a^k}{k!} + \frac{a^c}{c!}\frac{c}{c-a}}$, $W_q = C(c,a)/(c\mu - \lambda)$,
$P(W_q > t) = C(c,a)\, e^{-(c\mu-\lambda)t}$ [23]. M/G/1 mean wait (Pollaczek and Khinchine) [24]:
$W_q = \rho\, s\,(1 + c_s^2) / \big(2(1-\rho)\big)$ with $s$ the mean service time and $c_s^2$ the squared coefficient of variation
of service ($0$ deterministic, $1$ exponential).

### 7.1 Bun.serve (one event-loop thread)

One JS thread serves requests, so the model is one server ($c = 1$). The service rate must be measured, not guessed.
Measurement (`bun tools/serve-closed-loop.ts`: handler spins 0.2 ms, $C$ closed-loop `fetch` clients on loopback, 3 s per
level after 0.5 s warm-up, one run on the machine named above):

| Clients $C$ | 1 | 2 | 4 | 8 | 16 | 32 | 64 |
|---|---|---|---|---|---|---|---|
| Throughput $X$ (req/s) | 2456 | 3447 | 3558 | 3486 | 3817 | 3633 | 3576 |
| Mean latency $W$ (ms) | 0.407 | 0.578 | 1.124 | 2.294 | 4.191 | 8.803 | 17.863 |
| $X \cdot W$ (Little, equals $C$) | 1.00 | 1.99 | 4.00 | 8.00 | 16.00 | 31.98 | 63.88 |

Reading: Little's law holds to within 0.2% at every level; throughput saturates at $\mu \approx 3586$ req/s (mean of
$C \ge 2$), so the service time including HTTP is $s = 1/3586 = 0.2789$ ms (0.2 ms of the handler plus about 0.08 ms of
runtime); beyond the knee latency grows linearly, $W = C/X$: $64/3576 = 17.9$ ms (measured 17.863). An earlier ad-hoc run of the same
workload on the same machine saturated near 4600 req/s: throughput on a shared desktop varies by about 25%, so repeat
measurements (hyperfine-style: several runs, report mean and spread) before drawing conclusions.

Prediction at 80% utilisation, $\lambda = 0.8\mu = 2868.8$ req/s (the handler is deterministic, so $c_s^2 \approx 0$):
$W_q = 0.8 \times 0.2789 / (2 \times 0.2) = 0.558$ ms, $W = 0.837$ ms; the exponential assumption (M/M/1) would predict
$W = 1.394$ ms, $1.67\times$ too pessimistic. At $\rho = 0.95$ the same formula gives $W_q = 2.65$ ms: latency is dominated by
the queueing term near saturation, the reason to keep $\rho \le 0.7$ for latency targets.

Finite buffer, **assumed** $\mu = 5000$, $K = 128$: $\lambda = 4000$ ($\rho = 0.8$): $P_K = 7.9\times10^{-14}$, $L = 4.0$,
$W = 1.0$ ms. $\lambda = 6000$ ($\rho = 1.2$): $P_K = 0.1667$, $\lambda_{eff} = 5000$, $L = 123$, $W = 24.6$ ms.

### 7.2 Tokio server (multi-thread runtime)

**Assumed** $c = 8$ worker threads, service time $s = 2$ ms so $\mu = 500$ req/s per worker, $\lambda = 3200$ req/s:
$a = 6.4$, $\rho = 0.8$, $C(8, 6.4) = 0.4576$ (46% of requests wait), $W_q = 0.4576/(8 \times 500 - 3200) = 0.572$ ms,
$W = 2.572$ ms, $L_q = \lambda W_q = 1.83$. $P(W_q > 10\,\text{ms}) = 0.4576\, e^{-800 \times 0.01} = 1.5\times10^{-4}$.
Pooling effect: one worker at the same utilisation ($\lambda = 400$, $\mu = 500$) has $W = 1/(500-400) = 10$ ms, $3.9\times$
worse than the pooled 8-worker system. With no queue (reject when all busy, M/M/c/c) Erlang B at $a = 6.4$, $c = 8$
gives $14.4\%$ rejected: a queue absorbs bursts, so size it from section 5 rather than removing it. The model assumes a
shared queue and non-blocking handlers; work stealing approximates it, and every blocking call lowers $c$ (section 4).

## 8. Measuring (summary; procedure in the `perf-math` skill)

Closed loop (`serve-closed-loop.ts`, wrk, oha, bombardier) checks Little and finds $\mu$; open loop (fixed arrival rate)
is required for latency under load, because a closed loop slows its own arrivals when the server slows (coordinated
omission). Fit USL from at least 6 worker counts; check $R^2$ before trusting $N^\*$.

## References
1. Sustrik, "Structured concurrency", 2016: https://sustrik.github.io/250bpm/blog:71/ (mirror of 250bpm.com, dated 2016-02-07)
2. Smith, "Notes on structured concurrency, or: Go statement considered harmful", 2018: https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/
3. Elizarov, "Structured concurrency", 2018: https://elizarov.medium.com/structured-concurrency-722d765aa952
4. SE-0304 Structured concurrency: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0304-structured-concurrency.md
5. JEP 453 Structured Concurrency: https://openjdk.org/jeps/453
6. RFC 2394 async/await: https://rust-lang.github.io/rfcs/2394-async_await.html
7. `std::pin` and `std::task::Waker`: https://doc.rust-lang.org/std/pin/index.html
8. ECMAScript jobs and host-defined job queues: https://tc39.es/ecma262/#sec-jobs
9. Lemon, "Kqueue: a generic and scalable event notification facility", 2001: https://people.freebsd.org/~jlemon/papers/kqueue.pdf
10. I/O completion ports: https://learn.microsoft.com/en-us/windows/win32/fileio/i-o-completion-ports
11. Axboe, "Efficient IO with io_uring": https://kernel.dk/io_uring.pdf
12. Blumofe and Leiserson, "Scheduling multithreaded computations by work stealing", JACM 1999: https://doi.org/10.1145/324133.324234
13. Tokio, "Making the Tokio scheduler 10x faster", 2019: https://tokio.rs/blog/2019-10-scheduler
14. Little, "A proof for the queuing formula L = lambda W", Operations Research 1961: https://doi.org/10.1287/opre.9.3.383
15. Reactive Streams specification: https://www.reactive-streams.org/
16. Google SRE book, "Addressing cascading failures": https://sre.google/sre-book/addressing-cascading-failures/
17. Bronson et al., "Metastable failures in distributed systems", HotOS 2021: https://doi.org/10.1145/3458336.3465286
18. Dean and Barroso, "The tail at scale", CACM 2013: https://doi.org/10.1145/2408776.2408794
19. Denning and Buzen, "The operational analysis of queueing network models", ACM Computing Surveys 1978: https://doi.org/10.1145/356733.356735
20. Amdahl, "Validity of the single processor approach to achieving large scale computing capabilities", AFIPS 1967: https://doi.org/10.1145/1465482.1465560
21. Gustafson, "Reevaluating Amdahl's law", CACM 1988: https://doi.org/10.1145/42411.42415
22. Gunther, "A general theory of computational scalability based on rational functions", 2008: https://arxiv.org/abs/0808.1431
23. Kleinrock, "Queueing Systems, Volume 1: Theory", Wiley 1975 (M/M/1/K, Erlang B and C).
24. Pollaczek-Khinchine formula: Kleinrock [23], chapter 5 (the M/G/1 queue).
