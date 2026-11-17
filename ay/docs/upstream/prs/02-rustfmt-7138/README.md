# rustfmt #7138: keep a trailing `::self` in use trees

Status: DRAFT NOT PUBLISHED. Local patch only; nothing was pushed, opened or commented on rust-lang.

- Upstream issue: https://github.com/rust-lang/rustfmt/issues/7138 (open, no assignee, labels `C-bug`, `A-imports`, `I-invalid-code`, `S-has-mcve`; checked live on 2026-11-17).
- Base upstream SHA: `cc3bc53e69dd8f19232bb2b6aa52d1ccc3daf5dc` (rust-lang/rustfmt `main`, "Merge pull request #7252 from jieyouxu/jieyouxu/ci/rustup").
- Patch: `patch.diff` in this directory (`git format-patch -1 --stdout`, one commit on top of the base).
- Toolchain: `nightly-2026-11-02` (`rust-toolchain.toml`), `rustc 1.101.0-nightly (3e92e1a2d 2026-11-01)`.

## Problem

`UseTree::normalize` in `src/imports.rs` rewrote an unaliased trailing `::self` to its parent path. `use a::b::self;` only imports the type-namespace item `b`; `use a::b;` imports `b` from every namespace. With a same-named function in scope the formatted code no longer compiles (E0255). MCVE from the issue:

```rust
mod a { pub mod b {} pub fn b() {} }
fn b() {}
use a::b::self;   // rustfmt before: `use a::b;` -> E0255
```

The same normalization was already reported as #3568 (closed with the maintainer comment "we need to disable this normalization"), so the fix removes it instead of adding an option. Whether `self` is redundant cannot be decided syntactically (it depends on name resolution), so the unaliased form is always preserved.

## Design notes and open points for the reviewer

- Behaviour change: `use std::io::self;` and `use crate::lexer::self;` are no longer rewritten to `use std::io;` / `use crate::lexer;`. Two existing target expectations (`tests/target/imports/imports.rs`, `tests/target/imports/imports_granularity_default-with-dups.rs`) and the unit test `test_use_tree_normalize` were updated accordingly.
- `foo::self as bar` -> `foo as bar` is untouched (same namespace caveat applies to it, but it is outside the issue title and MCVE; mention as a possible follow-up).
- A previous attempt, PR #7144 "Preserve trailing self in imports" (author tayfuryldz), took the same approach and was closed unmerged on 2026-09-28 without visible review comments; the reason for closing is not known. Check it before submitting to avoid duplicate work.
- Not decided by the issue: whether maintainers want this on the stable default style edition path (label `X-impacts-stable-default-format`) or behind a style edition. This patch changes it unconditionally; confirm with a reviewer.

## PR title draft

`fix: don't normalize a trailing ::self away in use trees`

## PR body draft

Fixes #7138.

`use a::b::self;` imports only the type-namespace item `b`, while `use a::b;` imports every item named `b`. Normalizing the former into the latter can introduce a conflict with a same-named function or macro, so the formatted code stops compiling (see #7138 and #3568).

This removes the `foo::self -> foo` normalization in `UseTree::normalize` and keeps `foo::self as bar -> foo as bar`. Existing expectations that relied on the old rewrite are updated, and `tests/source/issue-7138.rs` / `tests/target/issue-7138.rs` cover the reported case (plus a `pub use` variant).

(The submitting human must rewrite this text in their own words, see policy.)

## Test added

- `tests/source/issue-7138.rs` and `tests/target/issue-7138.rs` (identical content: the MCVE plus `pub use a::c::self;`; the target file compiles with `rustc --edition 2024 --crate-type lib`).
- Updated: `tests/target/imports/imports.rs`, `tests/target/imports/imports_granularity_default-with-dups.rs`, unit test `imports::test::test_use_tree_normalize` in `src/imports.rs`.

## Commands run and results (all on <build-host>, in the rustfmt clone)

Before the fix (new tests added, `src/imports.rs` unchanged):

```
$ CFG_RELEASE_CHANNEL=nightly RUSTFLAGS="-D warnings" RUSTFMT_CI=1 cargo test --locked
Mismatch at tests/source/issue-7138.rs:13:
 fn b() {}

-use a::b::self;
-pub use a::c::self;
+use a::b;
+pub use a::c;
Ran 786 system tests.
assertion `left == right` failed: 1 system tests failed
Mismatch at tests/target/issue-7138.rs:13:   (same diff)
Ran 1057 idempotent tests.
assertion `left == right` failed: 1 idempotent tests failed
test result: FAILED. 235 passed; 2 failed; ...   (test::system_tests, test::idempotence_tests)
```

Only the two tests for the new files failed; no other fixture failed.

After the fix, first run (before updating the old expectations) showed the two expected knock-on changes, then they were updated:

```
-use std::io;                    tests/source/imports/imports.rs:27
+use std::io::self;
-use crate::lexer;               tests/source/imports/imports_granularity_default-with-dups.rs:1
+use crate::lexer::self;
imports::test::test_use_tree_normalize: left: a::self right: a
```

After the fix, final (the same command CI uses):

```
$ CFG_RELEASE_CHANNEL=nightly RUSTFLAGS="-D warnings" cargo run --locked --manifest-path ci/Cargo.toml build-and-test
test result: ok. 237 passed; 0 failed; ...      (rustfmt lib, includes system + idempotence tests)
test result: ok. 25 / 17 / 7 / 5 / 27 / 2 / 2 / 1 / 4 / 1 passed; 0 failed in the remaining targets
CI_EXIT=0   (build --all-features, test, config_proc_macro, diff_check all passed)
$ cargo fmt --check                                   -> exit 0, no output
$ cargo clippy --locked --manifest-path <pkg>/Cargo.toml  for ., ci, config_proc_macro, diff_check
                                                      -> exit 0 for all four, no warnings
```

## Policy

rust-lang LLM policy: https://forge.rust-lang.org/policies/llm-usage.html. It bans LLM-written PR text and requires pre-agreement with the reviewer for LLM-created code. The patch and the drafts above were produced with an LLM agent, so a named human must re-author the change and the PR text, fully understand it, and obtain the reviewer agreement before submission. Publishing (push, PR, comment) requires the owner's explicit yes. The commit author on <build-host> is a bot-style identity and must be replaced by the human submitter.

## Status

Verified on <build-host>: the new tests fail before the fix and pass after it; full CI test command, `cargo fmt --check` and clippy (all four CI packages) pass.

Unverified: only the Linux x86_64 `nightly` channel was run (CI also runs `stable` channel and macOS/Windows); the `integration` CI job (third-party crates) was not run; maintainer agreement on the unconditional behaviour change and on the reason PR #7144 was closed.

Ready for human review only; not ready to publish until the policy steps above are done.
