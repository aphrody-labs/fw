# Agent loop

Derived from `https://bun.com/blog/bun-in-rust`.

1. **Ground first**: read `MAP.tsv`, `LIFETIMES_AND_STYLE.md` and the relevant `SKILL.md` before forming a hypothesis.
2. **Trial before fan-out**: validate a structural change on 1 to 3 files before applying it across crates.
3. **Split review**: one implementer works a single disjoint crate or file scope; two independent reviewers try to break it (slice bounds, `debug_assert!` side effects, stubs, workaround comments, missing tests); one fixer applies the verified feedback.
4. **Compiler errors are the work queue**: per crate, `cargo check -p <crate>`, then `cargo clippy -p <crate> -- -D warnings`, then `cargo test -p <crate>`.
5. **Git**: commit explicit paths (`git add <file1> <file2>`); never `git stash` or `git reset --hard`; never skip or delete tests.
