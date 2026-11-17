# Recursion: cost, safety, schemes and termination

As of 2026-11-17. Numbers marked "measured" come from the programs in `docs/science/examples/` on an i7-13700F, Windows 11,
Bun 1.4.5 (JavaScriptCore), rustc 1.102.0-nightly (2026-11-11); the rest is derived on the page from stated inputs.

## 1. The cost model: stack depth

A non-tail call pushes a frame of $F$ bytes; recursion of depth $d$ needs $d \cdot F$ bytes. With stack size $S$ the maximum depth is
$d_{max} = \lfloor S/F \rfloor$ (minus the frames below `main`). Measured frame (`examples/stack-frame.rs`: one `[u64;4]` local, one call,
`-O`): $F = 112$ bytes. Derived for that function:

| Stack $S$ | Where | $d_{max} = \lfloor S/112 \rfloor$ |
|---|---|---|
| 8 MiB = 8,388,608 B | typical Linux main thread (`ulimit -s`) | 74,898 |
| 2 MiB | Rust default for spawned threads (`std::thread::Builder`) | 18,724 |
| 1 MiB | Windows default main-thread reserve | 9,362 |

Measured in JavaScriptCore (`examples/stack-depth.ts`): a small non-tail recursive function throws `RangeError` after 39,908 calls; a
`f() + 1` variant reached 53,207 (frame size depends on the JIT tier and on live values, so depth is not a constant of the language).
Consequence: an algorithm whose depth is $\Theta(n)$ on user input is a denial-of-service bug on every platform; depth must be
$O(\log n)$, bounded by a stated constant, or the recursion must be made explicit (section 2).

Depth bound for divide and conquer: if each call recurses into a part of size at most $n/2$, depth is at most $\lfloor \log_2 n \rfloor$
(induction on $n$). Quicksort that recurses into the smaller partition and loops on the larger therefore has depth $\le \log_2 n$ in
the worst case; the naive version has depth $n$ on sorted input [1].

Rust specifics: dropping a long `Box` linked list is recursive and overflows the stack at depth about $S/F$; implement `Drop`
iteratively [2]. The compiler itself recurses over deep syntax and types and grows its stack on the heap with the `stacker` crate [3];
a recursion limit (`#![recursion_limit = "128"]` by default) turns runaway macro and trait-solver recursion into an error [4].

## 2. Tail calls

A call is a tail call when its result is the caller's result. Elimination replaces `call; ret` by a jump, so depth is $O(1)$.
The languages differ on whether this is **guaranteed**:

| Runtime | Tail calls | Evidence |
|---|---|---|
| JavaScriptCore (Bun, Safari) | Guaranteed in strict-mode code (ES2015 proper tail calls) | measured: `g(n) = n === 0 ? 0 : g(n-1)` with $n = 10^7$ completes, 10,000,001 calls, no `RangeError`; non-tail $1 + h(n-1)$ overflowed at 39,908 |
| V8, SpiderMonkey | Not implemented | ES2015 feature shipped by JSC only [5] |
| Rust | Not guaranteed; optimisation only. Explicit guaranteed tail calls are `become` (RFC 3407, still an open PR, not accepted), unstable behind `explicit_tail_calls` (tracking issue #112788, LLVM backend implemented); status in `compilers-languages.md` | [6] |
| WebAssembly | `return_call` instructions (tail-call proposal), standardised | [7] |
| LLVM | `musttail` forces the transformation or fails the build | [8] |

Practical rule: write loops as loops. Use tail recursion only where the language guarantees it (strict JavaScript in Bun, Scheme,
Wasm) or where `become`/`musttail` states the requirement in the source, as in an interpreter's dispatch loop where each opcode
handler tail-calls the next [9]. Otherwise use a **trampoline** (return a thunk, loop until a value) or an explicit stack on the heap.
Mutual recursion and CPS are the cases where loops do not suffice and a guaranteed tail call is the only zero-overhead form.

## 3. Continuation-passing style

CPS makes every call a tail call by passing the rest of the computation $k$ explicitly (Plotkin 1975 [10], Steele 1978 [11]). For
lambda terms, with $\overline{e}\,k$ the translation of $e$ with continuation $k$:

$$\overline{x}\,k = k\,x,\qquad \overline{\lambda x.e}\,k = k\,(\lambda x.\lambda k'.\ \overline{e}\,k'),\qquad \overline{e_1 e_2}\,k = \overline{e_1}\,(\lambda f.\ \overline{e_2}\,(\lambda v.\ f\,v\,k)).$$

Uses: `async`/`await` is a restricted CPS where the compiler turns each continuation into a state-machine variant (see `async.md`
section 2); defunctionalising the continuations (Reynolds 1972 [12]) gives the explicit heap stack of section 2; compilers use CPS or
its equivalent, SSA, as an intermediate form [13]. Cost: continuations allocate closures; with guaranteed tail calls the stack stays
$O(1)$ but heap use is $\Theta(\text{depth})$, which is the same memory moved from stack to heap, with a much larger limit.

## 4. Memoisation and dynamic programming

A recursive function with overlapping subproblems and optimal substructure (Bellman's principle [14]) is evaluated in
$|\text{subproblems}| \times \text{cost per state}$ by memoisation (top-down) or tabulation (bottom-up). Example, Fibonacci
$F(n) = F(n-1) + F(n-2)$, $F(1)=F(2)=1$: the naive call count satisfies $C(n) = C(n-1) + C(n-2) + 1$, whose solution is
$C(n) = 2F(n+1) - 1$ (induction: $C(0)=1=2\cdot1-1$, $C(1)=1$). Inputs and results (measured in Bun, `fib` counting calls):

| Case | Calls | Check |
|---|---|---|
| naive $n = 32$ | 7,049,155 | $2F(33) - 1 = 2 \times 3{,}524{,}578 - 1$ |
| naive $n = 40$ (formula) | 331,160,281 | $2F(41) - 1 = 2 \times 165{,}580{,}141 - 1$ |
| memoised $n = 40$ | 79 | $2n - 1$ |

Naive growth is $\Theta(\varphi^n)$ with $\varphi = (1+\sqrt5)/2 \approx 1.618$. Bottom-up needs $O(1)$ stack and, when only the last $w$ states matter,
$O(w)$ memory. Top-down recursion keeps the stack depth equal to the dependency depth ($n$ here), so for large $n$ prefer bottom-up or an
explicit stack. Memo tables need a bound or an eviction policy when keys come from users.

## 5. Complexity of divide and conquer

For $T(n) = a\,T(n/b) + f(n)$ with $a \ge 1$, $b > 1$, let $c^\* = \log_b a$ (Bentley, Haken, Saxe 1980 [15]; Cormen et al. [16]):
if $f(n) = O(n^{c^\*-\epsilon})$ then $T = \Theta(n^{c^\*})$; if $f(n) = \Theta(n^{c^\*} \log^k n)$ then $T = \Theta(n^{c^\*} \log^{k+1} n)$;
if $f(n) = \Omega(n^{c^\*+\epsilon})$ and the regularity condition holds then $T = \Theta(f(n))$. Worked: merge sort $a=2,b=2,f=n$:
$c^\*=1$, case 2, $\Theta(n \log n)$; Karatsuba $a=3,b=2,f=n$: $c^\*=\log_2 3 = 1.585$, case 1, $\Theta(n^{1.585})$; binary search
$a=1,b=2,f=1$: $\Theta(\log n)$. The Akra-Bazzi theorem handles unequal splits [17].

## 6. Recursion schemes

Separate the recursion from the logic. For a functor $F$ let $\mu F$ be the least fixed point with $\mathrm{in} : F(\mu F) \to \mu F$.
A catamorphism (fold) is the unique $h$ with $h \circ \mathrm{in} = \alpha \circ F(h)$, written $h = (\!|\alpha|\!)$; an anamorphism is its dual
(unfold) $[\!(\gamma)\!]$; a hylomorphism $\mathrm{hylo}(\alpha,\gamma) = (\!|\alpha|\!) \circ [\!(\gamma)\!]$ runs without building the intermediate
structure. Fusion law: if $f \circ \alpha = \beta \circ F(f)$ then $f \circ (\!|\alpha|\!) = (\!|\beta|\!)$ [18]. Uniqueness is why folds over
finite data terminate: each step consumes a strictly smaller subterm (section 7). Rust: implement the fold as a loop over a flat arena
(`Vec<Node>` addressed by index, children before parents), which makes the fold stack-safe, cache-friendly and parallelisable;
use recursion over `Box` trees only with a depth bound.

## 7. Termination and well-founded induction

A relation $\prec$ on $A$ is well-founded iff it has no infinite descending chain, equivalently iff induction holds:
$\big(\forall x.\ (\forall y \prec x.\ P(y)) \Rightarrow P(x)\big) \Rightarrow \forall x.\ P(x)$. **Termination criterion**: if a measure
$\mu : \mathrm{Args} \to A$ into a well-founded set satisfies $\mu(\text{args of each recursive call}) \prec \mu(\text{args of the call})$ then
the function terminates (an infinite call chain would be an infinite descending chain). Examples: structural recursion on a finite
tree (subterm order); Euclid's gcd with $\mu(a,b)=b$ on $\mathbb{N}$; Ackermann
$A(0,n)=n+1$, $A(m+1,0)=A(m,1)$, $A(m+1,n+1)=A(m,A(m+1,n))$ with the lexicographic order on $(m,n)$, which is well-founded on
$\mathbb{N}\times\mathbb{N}$ and shows termination though $A$ is not primitive recursive (Ackermann 1928 [19]). Proof assistants require such a
measure (Lean and Coq, structural or `termination_by`), because a total language cannot allow non-terminating definitions without
becoming inconsistent under Curry-Howard (`limits.md`).

No algorithm can decide termination in general (halting problem, Turing 1936 [20]) nor any non-trivial semantic property of programs
(Rice 1953 [21]): compilers and linters bound recursion by syntactic rules or fuel (recursion limits, const-eval step limits, trait-solver depth),
never by a complete analysis. Treat every such limit as a design input: an overflow error from the limit is correct behaviour, not a bug to
raise the limit around.

## 8. Checklist (the `recursion-design` skill)

1. State the measure that decreases, and the maximum depth for the worst input. 2. If depth can be $\Theta(n)$ on untrusted input, use a loop, an
explicit stack or a guaranteed tail call. 3. If subproblems overlap, memoise or tabulate; bound the table. 4. Divide and conquer: recurse into the
smaller part first. 5. Fold over a flat arena, not a pointer tree, when the tree can be deep. 6. Implement `Drop` iteratively for recursive owned
structures. 7. Test with the deepest input (a list of $10^6$ nodes), not a typical one.

## References
1. Sedgewick, "Implementing Quicksort programs", CACM 1978: https://doi.org/10.1145/359619.359631
2. "Learn Rust With Entirely Too Many Linked Lists", Drop: https://rust-unofficial.github.io/too-many-lists/
3. rust-lang/stacker: https://github.com/rust-lang/stacker
4. The Rust Reference, `recursion_limit`: https://doc.rust-lang.org/reference/attributes/limits.html
5. ECMAScript 2015, tail position calls: https://262.ecma-international.org/6.0/#sec-tail-position-calls
6. RFC 3407 explicit tail calls (open pull request): https://github.com/rust-lang/rfcs/pull/3407 ; tracking issue https://github.com/rust-lang/rust/issues/112788
7. WebAssembly tail-call proposal: https://github.com/WebAssembly/tail-call
8. LLVM Language Reference, `musttail`: https://llvm.org/docs/LangRef.html#id327
9. Clang `musttail` attribute: https://clang.llvm.org/docs/AttributeReference.html#musttail
10. Plotkin, "Call-by-name, call-by-value and the lambda-calculus", TCS 1975: https://doi.org/10.1016/0304-3975(75)90017-1
11. Steele, "Debunking the 'expensive procedure call' myth", 1977: https://doi.org/10.1145/800179.810196
12. Reynolds, "Definitional interpreters for higher-order programming languages", 1972: https://doi.org/10.1145/800194.805852
13. Appel, "Compiling with continuations", Cambridge University Press 1992.
14. Bellman, "The theory of dynamic programming", Bull. AMS 1954: https://doi.org/10.1090/S0002-9904-1954-09848-8
15. Bentley, Haken, Saxe, "A general method for solving divide-and-conquer recurrences", SIGACT News 1980: https://doi.org/10.1145/1008861.1008865
16. Cormen, Leiserson, Rivest, Stein, "Introduction to Algorithms", chapter 4 (master theorem).
17. Akra and Bazzi, "On the solution of linear recurrence equations", 1998: https://doi.org/10.1023/A:1018373005182
18. Meijer, Fokkinga, Paterson, "Functional programming with bananas, lenses, envelopes and barbed wire", FPCA 1991: https://doi.org/10.1007/3540543961_7
19. Ackermann, "Zum Hilbertschen Aufbau der reellen Zahlen", Math. Annalen 1928: https://doi.org/10.1007/BF01459088
20. Turing, "On computable numbers", Proc. London Math. Soc. 1936: https://doi.org/10.1112/plms/s2-42.1.230
21. Rice, "Classes of recursively enumerable sets and their decision problems", Trans. AMS 1953: https://doi.org/10.1090/S0002-9947-1953-0053041-6
