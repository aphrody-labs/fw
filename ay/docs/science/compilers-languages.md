# Compiler and language blockers, read live on 2026-11-17

Sources: `gh api` on rust-lang/rust, rust-lang/goals (formerly rust-project-goals), rust-lang/cargo, oven-sh/bun, oven-sh/WebKit; rust-lang blog; RELEASES.md.
"Upd" is the GitHub `updated_at`. Items not confirmed from a primary source are marked **unverified**. Statuses change weekly: re-read the linked
issue before acting on a row. The theorems that explain why these problems are hard are in `limits.md`.

## 1. Rust

Baseline: stable **1.100.0** released 2026-11-12 [1]; beta 1.101 (2026-12-24 per releases.rs), nightly 1.102 (2027-02-04) [2]. The project goals for 2026 are a single
annual period (not 2026H1/H2) under `rust-lang/goals`, about 85 open tracking issues; flagship goals: cargo-script, Polonius alpha, const generics and const traits,
ergonomic ref-counting and async fn dyn dispatch, never type and `Try`, field projections and in-place init, build-std, next-gen trait solver [3].

| Blocker | Tracking | State on 2026-11-17 | Why it matters here |
|---|---|---|---|
| Parallel front end (`-Zthreads`) | [#113349](https://github.com/rust-lang/rust/issues/113349), goal [#121](https://github.com/rust-lang/goals/issues/121) | **On by default on nightly with 2 threads since `nightly-2026-11-14`** (PR #162848); opt out with `-Zunstable-options --jobs-frontend=1`; stable still sequential, no date. Blog numbers at 8 threads vs sequential: check -44%, debug -36%, opt -28% wall time. Diagnostic order is non-deterministic; memory grows with threads [4] | Our nightly builds are already parallel; measure memory on small hosts (`perf-math`) |
| Next-gen trait solver | [#107374](https://github.com/rust-lang/rust/issues/107374), goal [#113](https://github.com/rust-lang/goals/issues/113) | Coherence stable since 1.84; new solver on by default on nightly since 2026-08-21; stabilisation PR [#164209](https://github.com/rust-lang/rust/pull/164209) (`-Znext-solver=globally`) finished FCP 2026-10-24 but is `S-blocked` on #164055; the PR says stable would switch "starting in 1.102" (2027-02-04), merge timing **unverified**. Diesel `check full` overhead fell from 1.49x to 1.16x (Nethercote, 2026-11-04) [5] | Unblocks TAIT and return type notation; removes whole classes of trait-solver unsoundness |
| Polonius / NLL problem case #3 | goal [#118](https://github.com/rust-lang/goals/issues/118) | "Polonius alpha" enabled on nightly by PR #159343 (merged 2026-08-05, 1.99.0), `-Zpolonius=nll` disables it; problem case #3 still errors on stable. Mean +1.4% compile time over the top 20,000 crates, 95% of crates under +5%, worst case 2.5x on huge functions. Soundness issue [#153215](https://github.com/rust-lang/rust/issues/153215) open | Accepts more safe code; by Rice (`limits.md`) a complete checker is impossible, only a larger accepted set |
| Async drop | [#126482](https://github.com/rust-lang/rust/issues/126482), goal [#628](https://github.com/rust-lang/goals/issues/628) | Unstable (`async_drop`); main remaining work is `dyn`; PR #165266 (2026-11-01) reverted #164987 after a cycle-detection regression #165263; goal update 2026-10-05: no major implementation news | Without it, structured async cleanup needs explicit `close().await` (`async.md` section 1) |
| AFIT / RPITIT | PR #115822 | **Stable since 1.75 (2023-12-28)**; `dyn` and `Send` bounds on the returned future remain the gap | Generic code cannot yet say "this async method returns a `Send` future" without return type notation |
| Return type notation | [#109417](https://github.com/rust-lang/rust/issues/109417), goal [#646](https://github.com/rust-lang/goals/issues/646) | Unstable, `S-tracking-impl-incomplete`, blocked on the new solver | The fix for the `Send`-bound problem above |
| TAIT | [#63063](https://github.com/rust-lang/rust/issues/63063) | Unstable, same blocker | Named opaque future types |
| Async closures | PR #132706 | **Stable since 1.85 (2025-02-20)** | - |
| Async `dyn` dispatch | goal [#625](https://github.com/rust-lang/goals/issues/625) | Rough local prototype only (update 2026-10-15) | Trait objects of async traits |
| `gen` blocks, async iterator | [#117078](https://github.com/rust-lang/rust/issues/117078), [#79024](https://github.com/rust-lang/rust/issues/79024) | Unstable | Stream ergonomics |
| Generic const arguments | [#132980](https://github.com/rust-lang/rust/issues/132980), goal [#100](https://github.com/rust-lang/goals/issues/100) | Unstable under `gca_*` gates; `generic_const_exprs` [#76560](https://github.com/rust-lang/rust/issues/76560) is being abandoned in favour of them (goal update 2026-11-06); the old `min_generic_const_args` gate name is **unverified** | Array lengths computed from const params |
| Const traits | [#143874](https://github.com/rust-lang/rust/issues/143874), goal [#106](https://github.com/rust-lang/goals/issues/106) | Unstable; RFC 3762 open, "take III" rfcs#4008 open (upd 2026-11-12) | `const fn` over traits |
| Specialization | [#31844](https://github.com/rust-lang/rust/issues/31844), goal [#652](https://github.com/rust-lang/goals/issues/652) | Unstable, `S-tracking-design-concerns`; a lang+types design doc exists (2026-09-25); capability safety is the open design point; `min_specialization` is std-internal; 1.92 stopped specialising `Copy` in std because it was unsound | Do not use it in published code |
| Explicit tail calls `become` | RFC [#3407](https://github.com/rust-lang/rfcs/pull/3407) **open, not accepted**; tracking [#112788](https://github.com/rust-lang/rust/issues/112788) (upd 2026-11-03); goal [#634](https://github.com/rust-lang/goals/issues/634) | Feature `explicit_tail_calls`, LLVM backend implemented (LLVM 22 improves x86_64 tail calls); wasm needs the `tail-call` target feature; ppc `elf1` cannot tail-call across objects; fallback mechanism PR #165044 is a draft; an experimental `extern "tail"` ABI is planned. Cranelift and GCC support: **unverified** | Guaranteed constant-stack recursion in Rust (`recursion.md` section 2) |
| Never type `!` | PR #155499 | **Stabilised in 1.100.0 (2026-11-12)**; fallback is `!` on all editions; `Infallible` is an alias of `!` | - |
| Public/private dependencies | [#44663](https://github.com/rust-lang/rust/issues/44663), goal [#272](https://github.com/rust-lang/goals/issues/272) | Unstable, goal "needs contributor" | Semver hygiene in workspaces |
| Linker | PR #140525 | `rust-lld` is the default on `x86_64-unknown-linux-gnu` **stable since 1.90 (2025-09-18)** [6] | Link time of large binaries |
| Cranelift backend | goal [#639](https://github.com/rust-lang/goals/issues/639) | Nightly component `rustc-codegen-cranelift-preview`; Zed incremental builds 5-12% faster than LLVM (Feb 2026); first experiment (persistent caching daemon) regressed with all cores busy | Debug-build speed, not release |
| GCC backend | rustc_codegen_gcc | Readme still "WIP", needs patched libgccjit; status beyond that **unverified** | Targets LLVM lacks |
| `build-std` | goal [#274](https://github.com/rust-lang/goals/issues/274), cargo#16960 | Moving to an "opaque dependencies" design; the new UI comes before removing `-Zbuild-std` | Custom targets, OS work |
| Cargo build dir | cargo#17354 | `build.build-dir` stable since Cargo 1.91 (2025-10-30); a new layout shipped in 1.100 (re-stabilised 2026-08-19 after a revert) | Cache sharing, CI |
| Compile-time trend | [5] | Wall time -2.85% mean, max-RSS -4.46% (2026-09-28 to 2026-11-02); 560 of 561 instruction-count measurements improved since 2026-01-01 | perf.rust-lang.org itself is a JS shell: dashboard data **unverified** |

How the project works (teams, RFC and FCP, bors, crater, perf): `rust-upstream` skill. Rust for Linux: `os.md` section 3.

### What follows for our code
- Pin nightly only for a named feature (`become`, parallel front end opt-in, Polonius) and record the tracking issue next to the pin.
- Write `Send`-sensitive async traits with concrete futures (or `impl Future + Send` in the signature) until return type notation stabilises.
- Assume the trait solver and borrow checker will accept strictly more programs next year; never encode a workaround as an architecture.

## 2. Bun, JavaScriptCore, Zig

**Language state.** Bun left Zig: PR oven-sh/bun#30412 "Rewrite Bun in Rust" closed 2026-05-14; v1.3.14 (2026-05-13) still had `src/main.zig`, v1.4.0 (2026-08-20) has none
(`gh api 'repos/oven-sh/bun/contents/src/main.zig?ref=<tag>'`) [7]. `gh api repos/oven-sh/bun/languages` returns Rust 88.6 MB, C++ 14.4 MB, TypeScript 10.6 MB, C 1.06 MB, no Zig. Latest release
`bun-v1.4.4` (2026-10-22). Our fork checkout (HEAD 2026-11-17): 4,381 `.rs` and 0 `.zig` files under `src/`, 606 `.cpp`, 724 `.h`, 134 workspace members in `src/*`, Rust pinned to
`nightly-2026-11-14` (edition 2024), LLVM 23.1.1. Zig blockers that once mattered (incremental compilation, async/await removal, compile time) are historical; `oven-sh/zig` last pushed 2026-04-27.

**Open issue classes** (`is:issue is:open`, 2026-11-17, `gh api -X GET search/issues -f q='repo:oven-sh/bun is:issue is:open label:"<L>"' -f per_page=1 --jq .total_count`):

| Class | Open | Class | Open |
|---|---|---|---|
| all issues | 3,831 | bun install | 330 |
| all PRs | 5,946 | bundler | 258 |
| bug | 1,900 | bun:test | 160 |
| needs triage | 525 | windows | 141 |
| confirmed bug | 203 | linux / macOS | 82 / 68 |
| crash | 190 | bun:serve | 69 |
| needs investigate | 95 | jsc | 66 |
| performance | 89 | node:fs / node:http / web:fetch | 36 / 29 / 32 |
| regression | 36 | memory usage | 24 |
| `segfault` in title | 38 | `epoll` in body | 13 |

Flow: 510 issues opened and 207 closed in the 30 days to 2026-11-17, so the open set grows by about 300 a month. Recent crash families: JSC garbage-collector write barriers
(#47768) and the B3 register allocator racing `RegExp.prototype.test` on arm64 (#48292), AOT checker panics (#48908), Windows `EBUSY` on global install (#47962).
`good first issue`: 29. Labels that do not exist in the repo (`perf`, `segfault`, `http2`, `streams`) return 0; use `performance`, `node:http2`, `node:stream`.

**JavaScriptCore / WebKit** (`oven-sh/WebKit`, 455 open issues and PRs, pin `WEBKIT_VERSION` in `scripts/build/deps/webkit.ts`): the open PRs of 2026-11-09 to 2026-11-17 concentrate on
AOT compilation (#1069, #1068 arm64 return-address signing, #1066, #1058, #1046, #1034, #1027, #1022, #1020), ArrayBuffer borrow across detach (#1070), DFG/FTL `bind()` length and
name (#1051, #1023), heap accounting (#1062, #1044), URL parser conformance (#1056, #1053, #1026), error positions (#1057), module semantics (#1054, #1033), RegExp source retention
(#1031), Intl.Locale (#1035, #1029) and derived-constructor `this` (#1018, #1019). Reading: correctness and memory-accounting classes dominate; the single-threaded JS model
(`async.md` section 7.1) is a design property, not an open bug.

**OS-level ceilings for Bun** (`os.md`): readiness I/O on macOS, IOCP via libuv on Windows, epoll on Linux (io_uring use in the event loop is **unverified**).

## References
1. Rust 1.100.0: https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/
2. Rust release dates: https://releases.rs/
3. Rust project goals 2026: https://github.com/rust-lang/rust-project-goals/tree/main/src/2026
4. "Enabling the parallel front end on nightly", 2026-11-16: https://blog.rust-lang.org/2026/11/16/enabling-parallel-frontend-on-nightly/
5. Nethercote, "How to speed up the Rust compiler in November 2026": https://nnethercote.github.io/2026/11/04/how-to-speed-up-the-rust-compiler-in-november-2026.html
6. "rust-lld on 1.90.0 stable", 2025-09-01: https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/
7. oven-sh/bun: https://github.com/oven-sh/bun/pull/30412
