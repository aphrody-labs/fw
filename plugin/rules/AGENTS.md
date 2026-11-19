# Fairwind Unified Continuous Improvement Contract

- Fairwind (`C:\src\fairwind`) unifies first-party and third-party capabilities (`heretic`, `neon`/REA, `craft-libs`/`craft-pe`, `craft-anything`, `ghidra`, `tree-sitter`, `fairwind-abi`, `fairwind-agy`, `fairwind mcp`, `fairwind api`, autonomous `learn`, and `agy-knowledge`) behind one Fat-LTO + Bun AOT toolchain.
- Treat Google's Authenticode-signed upstream `agy.exe` (`%LOCALAPPDATA%\agy\bin\agy.exe`) strictly as read-only; build, verify, and promote Fairwind's native Fat-LTO `target/agy/agy.exe`, Bun AOT `--trim` `target/agy/fairwind.exe`, and Fat-LTO `fairwind_abi.dll` via `fairwind upgrade --apply`.
- Keep all in-repo code registered in `fairwind.targets.json`, audited in `target/manifest.json`, and continuously converged by the cross-platform boot-startup auto-restart daemon (`fairwind service install` / `fairwind daemon run --auto-apply --with-learn`).
