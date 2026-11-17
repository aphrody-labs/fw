# Fork deltas and sync status (2026-11-17)

Measured with `gh api repos/<fork>/compare/<upstream>:<branch>...<branch>` and local `git rev-list --left-right --count`.

| Fork | Upstream | Ahead of upstream | Behind upstream | Note |
|---|---|---|---|---|
| aphrody-labs/bun main | oven-sh/bun main | 4478 | 24 | diverged, merge base 237bba51e9 |
| aphrody-labs/rust main | rust-lang/rust main | 0 | 531 | plain mirror, no local patches |
| aphrody-labs/cargo master | rust-lang/cargo master | 0 | 86 | plain mirror |
| aphrody-labs/crates.io main | rust-lang/crates.io main | 0 | 42 | plain mirror |
| aphrody-labs/WebKit main | oven-sh/WebKit main | 0 | 6 | plain mirror |

Only bun carries work. The rust, cargo, crates.io and WebKit forks have nothing to upstream.

## bun sync result: not merged, conflicts

`git merge-tree --write-tree HEAD upstream/main` (no working-tree change) reports three content conflicts, all Windows code where upstream and the fork rewrote the same functions:

| File | Hunks | Cause |
|---|---|---|
| src/runtime/cli/upgrade_command.rs | 3 | fork 34433a90fb unpacks the Windows release in-process (bun_archive::zip); upstream `bun_core::zstr!` and `tmpdir_z` changes (#49160 and earlier) touch the same lines |
| src/sys/lib.rs | 4 | fork 037b86b86e `nt_root_directory(dir, path, syscall)` (empty path gives ENOENT); upstream (Windows bun_sys refactor, #49094, #49166) `nt_root_directory(dir, path) -> Option<HANDLE>` |
| src/sys/windows/mod.rs | 1 | fork 037b86b86e keeps the `UNICODE_STRING`/`OBJECT_ATTRIBUTES` build; upstream routes it through `NtCreateFile(dir, name, options)` in bun_sys |

Everything else auto-merges. No merge was forced. Resolution needs a Windows build (the conflicting code is `cfg(windows)`), and upstream already moves the direct Win32 calls into `bun_sys`, so the fork's ENOENT-on-empty-path rule has to be re-expressed inside upstream's new `NtCreateFile` wrapper.

## Fork commits worth upstreaming

Source: `git log --no-merges upstream/main..HEAD -- src test packages scripts/build`, restricted to non-Aphrody-specific code.

| Commit | Change | Status upstream | Testable on Linux |
|---|---|---|---|
| 34433a90fb | `bun upgrade` on Windows unpacks the zip in-process (PowerShell 5.1 with a PS7 PSModulePath extracts nothing and exits 0); canary download checked against the releases API digest | original work, no upstream PR found by `gh search prs` | no (Windows) |
| ed0fe71de8 | `bun completions` prints bash, zsh, fish scripts on Windows when `$SHELL` is set (fixes #25042) | original, no upstream PR found by `gh search prs` | no (Windows) |
| 65a7047411 | `Bun.Image#raw({channels})` terminal | open upstream PR #49107 covers it, nothing to submit | n/a |
| 3d31f4b98f, 4835f16198, 5431418c76, 037b86b86e, 4b7843f610 | backports of upstream PRs #39760, #48698, #45410, #38052, #45742 (open upstream) | already upstream PRs, nothing to submit | n/a |
| 01f9d92783, c141095b64 and the `migrate`, `modules` trees | fork-specific | not upstreamable | n/a |

The five backports should be dropped from the fork once upstream merges them, which removes part of the conflict surface above.
