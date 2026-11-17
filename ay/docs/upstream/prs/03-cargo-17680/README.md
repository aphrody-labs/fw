# cargo #17680: do not suggest `--future-incompat-report` for commands that reject it

Status: DRAFT NOT PUBLISHED. Nothing was pushed, filed or commented on rust-lang.

## Upstream issue

- https://github.com/rust-lang/cargo/issues/17680
  "`cargo run` suggests adding `--future-incompat-report`, which does not work"
- State checked live on 2026-11-17: open, no assignee, labels `C-bug`, `S-triage`.
- S-label status: still `S-triage`. Cargo reviews only `S-accepted` issues, so a
  human must first ask a cargo team member to accept it (triagebot
  `@rustbot label +S-accepted` is team-only).
- The reporter (mkeeter) wrote "If the maintainers want to point me to their
  preferred solution, I'd be willing to pick this up." Not assigned, but the
  reporter has signalled interest; a human should check with them before
  submitting.
- Design: the issue offers two options, "add support for the flag to `cargo run`"
  or "remove that suggestion when invoked through the `run` subcommand".
  No maintainer has chosen. This patch implements the second (smaller) option;
  the choice is unconfirmed by maintainers.
- Related: #10848 (`cargo install` cannot use `cargo report` at all; a different
  bug, left untouched).

## Base

- Upstream rust-lang/cargo `master` at `8ca2d0b260a7072fcecb2666d3e9516c00379aab`
  (`refactor(context): miscellaneous loader cleanup (#17747)`).
- Patch commit on the clone: `48b2660cdef0b44f517a91e20580b65f777117b5`
  (`patch.diff` is `git format-patch -1 --stdout`).

## Files changed

- `src/compiler/build_config.rs`: new `BuildConfig::future_incompat_report_flag_available`
  (default `false`).
- `src/util/command_prelude.rs`: `compile_options` sets it from whether the
  command defines the `future-incompat-report` arg (`self._contains(..)`).
  Only `build`, `check`, `rustc`, `test` define it.
- `src/compiler/future_incompat.rs`: the note says `use the option
  `--future-incompat-report`, or ` only when the flag is available; otherwise
  it only says `run `cargo report future-incompatibilities --id N``.
- `tests/testsuite/future_incompat_report.rs`: new test
  `no_flag_suggestion_for_unsupported_command`. Existing snapshots were not
  changed (all existing expectations are for `cargo check`/`build`/`test`).

## Test added

`future_incompat_report::no_flag_suggestion_for_unsupported_command`: runs
`cargo run` on a bin with `RUSTFLAGS=-Zfuture-incompat-test`, expects
`[NOTE] to see what the problems were, run `cargo report future-incompatibilities --id 1``,
then checks `cargo run --future-incompat-report` is rejected as an unexpected argument.

## PR title and body draft

Draft only. Per policy below a human must write the final text themselves.

Title: `fix(report): Only suggest --future-incompat-report where accepted`

Body:

> ### What does this PR try to resolve?
>
> The future-incompat note suggests `--future-incompat-report` for every
> command, but only `build`, `check`, `rustc` and `test` accept it.
> For example `cargo run` prints the suggestion and then fails with
> `unexpected argument '--future-incompat-report'`.
>
> The note now suggests the option only when the command defines it, and
> otherwise only suggests `cargo report future-incompatibilities --id N`.
>
> Fixes #17680
>
> ### How to test and review this PR?
>
> `cargo test --test testsuite -- future_incompat_report`. The new test fails
> without the `src/` change. `BuildConfig` defaults the new field to `false`,
> so callers that build a `BuildConfig` without command-line args (for
> example `cargo package` verification) do not suggest the flag either.

## Commands run (on <build-host>, clone of upstream master)

Toolchain: default nightly-2026-09-15 is too old for current master (E0658
`trim_prefix_suffix`, MSRV 1.100), so `RUSTUP_TOOLCHAIN=nightly-2026-11-16`
(`rustc 1.102.0-nightly (5f4effd57 2026-11-13)`) was used, after
`rustup component add rustc-dev llvm-tools-preview` on it.

```
git pull --ff-only                     # Already up to date
export RUSTUP_TOOLCHAIN=nightly-2026-11-16-x86_64-unknown-linux-gnu CFG_DISABLE_CROSS_TESTS=1
cargo test --test testsuite -- future_incompat_report
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps --keep-going
cargo test --test testsuite -- install:: run:: build_script:: warn_on_failure
```

### Before the fix (new test added, `src/` untouched): FAILS

```
test future_incompat_report::no_flag_suggestion_for_unsupported_command ... FAILED
...
- [NOTE] to see what the problems were, run `cargo report future-incompatibilities --id 1`
+ [NOTE] to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 4491 filtered out
```

### After the fix: PASSES

```
running 12 tests
test future_incompat_report::no_flag_suggestion_for_unsupported_command ... ok
(11 other future_incompat_report tests ok, including test_single_crate, test_multi_crate, incompat_in_local_crate)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 4491 filtered out; finished in 3.78s
```

- `cargo fmt --all --check`: exit 0, no output.
- `cargo clippy --workspace --all-targets --no-deps --keep-going` (as `.github/workflows/main.yml`): exit 0, no warnings.
- `cargo test --test testsuite -- install:: run:: build_script:: warn_on_failure`:
  `test result: ok. 321 passed; 0 failed`.

Snapshots were written by hand (not `SNAPSHOTS=overwrite`); no existing snapshot
needed to change.

## Applicable policy

rust-lang LLM policy: https://forge.rust-lang.org/policies/llm-usage.html
bans LLM-written PR text and requires pre-agreement with the reviewer for
LLM-created code. Therefore a named human must re-author this change, understand
every line, and write the PR text; publishing needs the owner's explicit yes.
Also needed first: a cargo team member must set `S-accepted` and, ideally,
confirm the chosen option (drop the suggestion versus accept the flag in `run`).

## Status

Ready locally, not for submission: the test fails before and passes after on <build-host>.
Unverified: the full cargo testsuite (only the targeted filters above ran), CI on
other platforms (Windows/macOS), maintainer agreement on the design, and
`S-accepted`.
