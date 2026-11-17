# ay-patch

The `apply_patch` file-edit format that coding models emit, as a pure Rust library (no async runtime, no OS
specific code, builds for `wasm32-unknown-unknown`):

```text
*** Begin Patch
*** Update File: src/lib.rs
@@ fn main()
-    println!("old");
+    println!("new");
*** End Patch
```

- `parse_patch` parses a complete patch document into hunks (`Add File`, `Delete File`, `Update File` with
  optional `Move to`).
- `StreamingPatchParser` parses a patch while it is still being generated.
- `apply_patch` applies hunks over a `PatchFileSystem` (`StdFileSystem` is backed by `std::fs`). Context is
  located with `seek_sequence`, a fuzzy line search (exact, trailing-whitespace, trim, then Unicode-normalised)
  so ASCII-authored diffs still apply to sources with typographic dashes, curly quotes or no-break spaces.
- `unified_diff` renders a line preview of what changed.

The format is inspired by the OpenAI Codex apply-patch tool (Apache-2.0) and is re-implemented here; no code
of that project is included.

Legacy published name: `aphrody-patch` (crates.io, 1.4.7). This crate is the restored source under the
repository's own name; it is not published.

## Test

```sh
cargo test -p ay-patch
cargo clippy -p ay-patch --all-targets -- -D warnings
```
