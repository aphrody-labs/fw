# Bun-in-Rust Autonomous Agent Loop (Split Context + Adversarial Review)

Derived from `https://bun.com/blog/bun-in-rust`:

1. **Ground & Serialize First**: Read `MAP.tsv` + `LIFETIMES_AND_STYLE.md` + relevant `SKILL.md` before forming any hypothesis.
2. **Trial Run Before Fan-Out**: Validate any structural pattern on 1–3 files before scaling across crates.
3. **Split Context Review (1 Implementer → 2 Adversarial Reviewers → 1 Fixer)**:
   - Implementer writes minimal zero-abstraction code for a single disjoint crate/file scope.
   - Two independent reviewers try to break it (checking memory/slice bounds, `debug_assert!` side effects, stubs, workaround comments, and missing tests).
   - Fixer applies verified feedback.
4. **Compiler Errors as a Work Queue**: Fix crate-by-crate (`cargo check -p <crate>` → `cargo clippy -p <crate> -- -D warnings` → `cargo test -p <crate>`).
5. **Git Isolation**: Commit explicit paths only (`git add <file1> <file2>`); never `git stash` or `git reset --hard`; never skip or delete tests.
