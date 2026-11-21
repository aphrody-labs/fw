# Upstream starter issues (verified 2026-11-17)

Source data: `issues.toml` (same directory). Each row was checked live: state OPEN, no assignee, no open cross-referenced PR, no standing claim comment, recent activity. "Compiler build" = needs a self-built rustc (`./x` stage build); none of the 20 does. Bun items need a full `bun bd` debug build instead (about 10 GB, 10-30 min).

PATCH = one of the 6 best first patches (small, well specified, testable with the repo's own suite on Linux with Rust nightly).

| Repo | # | Title | Labels | URL | Created | Updated | Why it fits | Size | Compiler build |
|---|---|---|---|---|---|---|---|---|---|
| rust-lang/rust-clippy | 18066 | large_stack_arrays lints the array of an associated const (free const is not linted) | C-bug, I-false-positive | https://github.com/rust-lang/rust-clippy/issues/18066 | 2026-11-14 | 2026-11-14 | PATCH. Full repro, pure lint logic | S | n |
| rust-lang/rust-clippy | 18054 | default_numeric_fallback flags a literal in an enum struct variant field with a pinned type | C-bug, I-false-positive | https://github.com/rust-lang/rust-clippy/issues/18054 | 2026-11-13 | 2026-11-13 | PATCH. Struct field works, variant field does not | S | n |
| rust-lang/rust-clippy | 18043 | disallowed_methods fires inside a macro from another crate | C-bug, good first issue, I-false-positive | https://github.com/rust-lang/rust-clippy/issues/18043 | 2026-11-11 | 2026-11-13 | Macro spans; no LLMs on good first issue; design open | M | n |
| rust-lang/rust-clippy | 17991 | manual_ignore_case_cmp: --fix turns false into true | C-bug, L-suggestion, I-suggestion-causes-error | https://github.com/rust-lang/rust-clippy/issues/17991 | 2026-11-06 | 2026-11-10 | Behavior-changing suggestion; prior PR #18022 closed (suspected LLM) | S | n |
| rust-lang/cargo | 17680 | `cargo run` suggests `--future-incompat-report`, which does not work | C-bug, S-triage | https://github.com/rust-lang/cargo/issues/17680 | 2026-11-03 | 2026-11-03 | PATCH. Wrong hint text; comment first (not S-accepted) | S | n |
| rust-lang/cargo | 17634 | Undetected changes to non-UTF-8 env vars used at build time | C-bug, S-triage | https://github.com/rust-lang/cargo/issues/17634 | 2026-10-23 | 2026-10-23 | PATCH. Fingerprint stores `Option<String>`; shell repro | M | n |
| rust-lang/cargo | 17704 | `--features pkg/feature` ignored when package has a self dev-dependency | C-bug, S-triage | https://github.com/rust-lang/cargo/issues/17704 | 2026-11-09 | 2026-11-09 | Resolver/features, minimal manifest | M | n |
| rust-lang/cargo | 17646 | warning on unspecified edition missing suggestion | C-bug, A-diagnostics, S-needs-design | https://github.com/rust-lang/cargo/issues/17646 | 2026-10-27 | 2026-10-27 | Manifest diagnostics; agree design first | M | n |
| rust-lang/rust-analyzer | 23628 | Missing `parameter` semantic token modifier in `format` macro | E-easy, E-has-instructions, C-bug, A-highlighting | https://github.com/rust-lang/rust-analyzer/issues/23628 | 2026-11-07 | 2026-11-07 | Maintainer instructions given; no AI-authored code allowed here | S | n |
| rust-lang/rust-analyzer | 23603 | Cycles in crate graph cause `View Crate Graph` to hang | A-vscode, C-bug, A-ide | https://github.com/rust-lang/rust-analyzer/issues/23603 | 2026-11-02 | 2026-11-02 | Graph traversal in IDE layer | S | n |
| rust-lang/rust-analyzer | 23356 | panic: entered unreachable code: inference diagnostic in desugared expr | C-bug, I-panic | https://github.com/rust-lang/rust-analyzer/issues/23356 | 2026-09-13 | 2026-11-12 | hir-ty panic; claims released; must cite rustc code | M | n |
| rust-lang/rustfmt | 7138 | Rustfmt removes trailing `::self` unconditionally | C-bug, A-imports, I-invalid-code, S-has-mcve | https://github.com/rust-lang/rustfmt/issues/7138 | 2026-09-26 | 2026-10-25 | PATCH. Changes semantics; MCVE; src/imports.rs | M | n |
| rust-lang/rustfmt | 6896 | Weird whitespace between code and in-line comments | C-bug, I-poor-formatting, A-comments, S-has-mcve | https://github.com/rust-lang/rustfmt/issues/6896 | 2026-05-13 | 2026-09-25 | MCVE, stable defaults reproduce | M | n |
| rust-lang/miri | 5242 | Miri can't detect clashing static/function shim symbols in all cases | C-bug, A-shims, I-misses-UB | https://github.com/rust-lang/miri/issues/5242 | 2026-08-04 | 2026-08-04 | PATCH. Maintainer: easy half; `extern_statics` | S | n |
| rust-lang/miri | 5256 | pthread primitives: provenance passed to "init" must remain valid | C-bug, A-shims, I-misses-UB | https://github.com/rust-lang/miri/issues/5256 | 2026-08-16 | 2026-08-16 | Concurrency shim precision; discuss first | M | n |
| rust-lang/crates.io | 15161 | Adding Trusted Publisher fails without an email address | (none) | https://github.com/rust-lang/crates.io/issues/15161 | 2026-11-09 | 2026-11-09 | Backend controller + UI hint; needs Postgres | S | n |
| rust-lang/crates.io | 14747 | Blocked URLs should block links in READMEs as well | (none) | https://github.com/rust-lang/crates.io/issues/14747 | 2026-09-21 | 2026-09-21 | README render + re-render of one crate | M | n |
| rust-lang/rust | 89040 | Unclear documentation around HashMap iteration order | E-help-wanted, A-docs, T-libs | https://github.com/rust-lang/rust/issues/89040 | 2021-09-17 | 2026-11-13 | Doc-only, but a libs policy question; weak fit | S | n |
| oven-sh/bun | 48797 | CSS Modules ReferenceError with HMR (import_Button_module undefined) | bug, needs triage, bake:dev, css | https://github.com/oven-sh/bun/issues/48797 | 2026-11-13 | 2026-11-13 | Rust bundler/dev-server; reproduced on Linux by bot; no PR | M | n (needs `bun bd`) |
| oven-sh/bun | 48531 | Windows: recursive rm spins on delete-pending file | bug, windows, node:fs | https://github.com/oven-sh/bun/issues/48531 | 2026-11-11 | 2026-11-11 | Windows sys code in node_fs.rs; needs Windows host | M | n (needs `bun bd`) |

## The 6 patch candidates

| Issue | Hint |
|---|---|
| clippy#18066 | `clippy_lints/src/large_stack_arrays.rs`: skip arrays in associated consts like free consts; UI test + `cargo bless` |
| clippy#18054 | `clippy_lints/src/default_numeric_fallback.rs`: treat enum struct-variant fields like struct fields (edit site unverified) |
| cargo#17680 | `src/compiler/future_incompat.rs`: do not suggest the flag for commands that reject it; `tests/testsuite/future_incompat_report.rs` |
| cargo#17634 | `src/compiler/fingerprint/mod.rs`: `RerunIfEnvChanged { val: Option<String> }` is lossy; keep OsString/bytes |
| rustfmt#7138 | `src/imports.rs`: `UseSegmentKind::Slf` normalization removes a non-redundant trailing `self` |
| miri#5242 | `src/machine.rs` `extern_statics`: also check static shims when resolving a function symbol; `./miri test` |

## Selection notes

- Claimed or PR-covered and therefore rejected (examples): clippy#17893 and #17929 (claimed), cargo#7813 (claimed 2026-11-16), #8703 (draft PR #17544), #11846, #15870, #17652, rust-analyzer#23144, #23153, #23237, #23538, rustfmt#6539, miri#5352 (soft claim), miri#3179 and #5569 (many parallel claims), rust#159751 (dozens of claims) and #159686 (claimed, PR open).
- Bun: nearly every fresh `confirmed bug` already has an open PR within hours (the maintainers' bot `robobun` opens them), and the `good first issue` list is saturated with claims and PRs. The two Bun rows above were the only ones found with no PR and no claim; recheck right before starting.
- cargo: `CONTRIBUTING.md` says only issues marked S-accepted are reviewed; the cargo rows are S-triage or S-needs-design, so ask a team member to accept the issue before coding.
- `cargo` and `rust-lang/rust` have almost no unassigned E-easy items; the cargo `E-easy` label has a single open issue, assigned.
