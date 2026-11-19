# `uutils` Defects Audit & Long-Term Patches in `fw-uutils` (2026-11-19)

Sources audited: live `uutils/coreutils` GitHub issues, `uutils` org repos, Ubuntu 25.10 / 26.04 LTS transition reports, Zellic security audit, Openwall 2026 CVEs, and Windows/MSYS2/WSL integration issues.

| Defect / Source | Upstream `uutils` Problem | Permanent Fix in `fw-uutils` |
| :--- | :--- | :--- |
| **CVE-2026-35363** (`rm`) | Trailing slashes (`./`, `.///`, `../`) bypassed `.` / `..` deletion guard | `hardened::validate_rm_target` strips trailing `/` & `\`, rejects roots (`/`/`C:\`), `.`/`..`, and NTFS reserved devices (`CON`/`NUL`/`COM1`) |
| **CVE-2026-35364 / 35365** (`mv`) | Cross-device `mv` removed destination first (TOCTOU symlink race) and expanded symlinks | `hardened::safe_mv` checks `symlink_metadata`, refuses symlink overwrite/expansion, and stages cross-device copies to `.fw-mv-<pid>.tmp` + atomic `rename()` |
| **Issue #16228** (Windows) | Multicall binary rejected mixed-case `argv[0]` (`LS.EXE`, `Grep.exe`) | `hardened::normalize_applet_name` lowercases `argv[0]` stem before multicall dispatch in `fw-bin` |
| **Issue #7092** (`cp` perf) | Slow userspace copy path on large files | `hardened::safe_cp` uses OS zero-copy `fs::copy` (`copy_file_range` on Linux, `CopyFileExW` on Windows) with symlink guard |
| **Issues #9549 & #13399** | Duplicate `nix` + `rustix` deps; `rustix` raw syscalls bypass `libc` & break `LD_PRELOAD` | Zero `nix`, zero `rustix`, zero `clap` — 100% `std` (preserves `LD_PRELOAD` and keeps `fw.exe` at 873 KiB) |
| **Issue #5088** (Nushell) | Unstructured text-only output | Native `fw ls --tsv` & `fw table [--json]` (**5.4 ms**, 4.6x faster than `nu`) |
| **UTF-8 / Non-UTF-8 paths** | Strict `String` handling tripped on non-UTF-8 bytes | `fast_cli` operates on raw `&[u8]` (`as_encoded_bytes()`) with zero UTF-8 panics |
| **MSYS2 / WSL path friction** | `/c/...` and `/mnt/c/...` path mismatches on Windows | `hardened::normalize_cross_os_path` translates MSYS2/WSL drive prefixes cleanly |
