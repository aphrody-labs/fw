# rust-clippy #18066: large_stack_arrays on associated consts

Status: DRAFT NOT PUBLISHED. Nothing was pushed, opened or commented on any remote.

- Upstream issue: https://github.com/rust-lang/rust-clippy/issues/18066 (open, no assignee, 0 comments, labels `C-bug`, `I-false-positive`; re-read live on 2026-11-17; a search for PRs mentioning 18066 returned none)
- Base upstream SHA: `b87acaf4d74921dca2d003e3e48045ea34e033bd` (`fix(missing_errors_doc): exempt never-like error types (#18060)`, upstream `master`, fast-forwarded on fr1)
- Patch: `patch.diff` in this directory (`git format-patch -1 --stdout`, one commit, applies on the base SHA above)
- Toolchain: `nightly-2026-11-14` from `rust-toolchain.toml` (auto-installed on fr1)

## Applicable policy

rust-lang LLM policy: https://forge.rust-lang.org/policies/llm-usage.html (adopted by Clippy, see `CONTRIBUTING.md` "LLM policy" and `book/src/development/llm_usage.md` in the clone). It bans LLM-written PR text and requires pre-agreement with the reviewer for LLM-created code. This patch and the PR text below were drafted with LLM assistance, so before any submission:

1. A named human must re-author the code and the PR title/body in their own words and understand every line.
2. Pre-agreement with the reviewer is needed, and the PR description must disclose LLM use as upstream commits do (`LLM assistance: ...`).
3. Publishing (claiming the issue with `@rustbot claim`, pushing a fork branch, opening the PR) needs the owner's explicit yes. None of it was done.

The commit author in `patch.diff` is the local machine identity, not the future submitter.

## PR title draft

`fix(large_stack_arrays): don't lint arrays in associated consts`

## PR body draft

Fixes #18066

`large_stack_arrays` already skips arrays inside a free `const` and inside a `static`, because their value is evaluated at compile time and takes no stack frame. The same arrays in an associated const (inherent impl, trait default, trait impl) were still reported, with a `vec![...].into_boxed_slice()` suggestion that a const cannot use.

The ancestor check in `LargeStackArrays::check_expr` that already exempts `ItemKind::Static` now also exempts `ImplItemKind::Const` and `TraitItemKind::Const`. Free consts keep being handled by the existing `const_item_counter`. A counter pair for associated consts was not possible: `LateLintPass` has `check_impl_item_post` but no `check_trait_item_post`, so the ancestor walk was used for both kinds.

Arrays in ordinary function bodies are unchanged. Arrays nested inside an associated const's initializer (for example in a block or closure) are skipped, the same as for a `static`.

changelog: [`large_stack_arrays`]: No longer lints arrays in associated consts

## What changes

- `clippy_lints/src/large_stack_arrays.rs`: extends the `hir_parent_iter` `matches!` with `Node::ImplItem` / `Node::TraitItem` whose kind is `Const(..)`; imports `ImplItem`, `ImplItemKind`, `TraitItem`, `TraitItemKind`.
- `tests/ui/large_stack_arrays.rs`: adds an inherent impl with two associated consts (literal-size array, repeat expression), a trait with a default associated const and a required one, and an impl providing the required one. All arrays exceed the default 16384-byte threshold (5000 `u32`, 20 000 `u8`), so they would lint if not exempted. The existing `let` and tuple cases still assert that local arrays are reported.
- `tests/ui/large_stack_arrays.stderr`: line-number shifts only (diff below the new test block); no new expected diagnostics.

## Commands run on fr1 and observed output

All commands ran in the clone on fr1 (`upstream-prs/rust-clippy`), one at a time, heavy builds on fr1 only.

### Before the fix (new test cases added, lint source at the base revision)

```
$ TESTNAME=large_stack_arrays cargo uitest
tests/ui/large_stack_arrays.rs ... FAILED
error: actual output differed from expected
 error: allocating a local array larger than 16384 bytes
-  --> tests/ui/large_stack_arrays.rs:38:14
+  --> tests/ui/large_stack_arrays.rs:27:47
+LL |     const INHERENT_LITERAL: &'static [u32] = &[0; 5000];
+error: allocating a local array larger than 16384 bytes
+  --> tests/ui/large_stack_arrays.rs:28:46
+LL |     const INHERENT_REPEAT: &'static [u32] = &[7; 5000];
+  --> tests/ui/large_stack_arrays.rs:32:43
+LL |     const TRAIT_DEFAULT: &'static [u8] = &[0; 20_000];
+  --> tests/ui/large_stack_arrays.rs:37:44
+LL |     const TRAIT_REQUIRED: &'static [u8] = &[1; 20_000];
test result: FAIL. 1 failed; 1237 filtered out
```

The four unexpected diagnostics are the four associated consts; the free const and static cases already in the file are not reported.

### Fix applied, stderr regenerated

```
$ TESTNAME=large_stack_arrays cargo uibless
tests/ui/large_stack_arrays.rs ... ok
test result: ok. 1 passed; 1237 filtered out
```

`git diff tests/ui/large_stack_arrays.stderr` reviewed: 12 line-number updates and nothing else.

### After the fix

```
$ TESTNAME=large_stack_arrays cargo uitest
tests/ui/large_stack_arrays.rs ... ok
test result: ok. 1 passed; 1237 filtered out
```

### Full gates (all exit code 0)

```
$ cargo dev fmt          # exit 0, no diff left
$ cargo uitest           # test result: ok. 1897 passed; 5 ignored / ok. 195 passed / ok. 52 passed / ok. 1 passed
$ cargo dev dogfood      # exit 0 (tests/dogfood.rs: clippy lints its own workspace crates, no warnings)
$ cargo test             # exit 0, 19 `test result: ok` lines, no failures
```

## Status

Ready for human re-authoring: the new test fails before the fix (4 unexpected diagnostics, one per associated const) and passes after, and fmt, uitest, dogfood and cargo test pass on fr1.

Unverified:

- The exact reproducer from the issue (`cargo clippy` on the gist crate with `array-size-threshold = 16`) was not run; the UI test uses the default threshold with larger arrays covering the same three associated-const forms.
- The patch was not checked against a newer `master` than the base SHA above.
- Not reviewed by a human, and the reviewer pre-agreement required by the LLM policy does not exist.
