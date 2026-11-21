# Operating-system blockers for asynchronous and systems code

Read live on 2026-11-17 (commands and URLs inline; anything not confirmed from a primary source is marked "unverified").

## 1. io_uring: performance against attack surface

Model and syscall arithmetic: `async.md` section 3 (epoll $2 + 1/E$ syscalls per request, io_uring $2/B$).

Security record:
- Google (June 2023): 60% of the kCTF VRP submissions exploited io_uring, about 1M USD paid for io_uring bugs; ChromeOS disabled it,
  Android restricts it by seccomp-bpf, GKE Autopilot was "investigating disabling by default" [1]. No 2026 Google statement on Android or
  ChromeOS was found: **unverified** for 2026.
- NVD keyword search `io_uring` (`https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=io_uring&pubStartDate=..&pubEndDate=..&resultsPerPage=1`,
  120-day windows, queried 2026-11-17) [2]: 166 CVEs all time; 2025: 7 + 10 + 23 + 0 = 40; 2026 through 2026-11-17: 9 + 27 + 22 = 58 (the window
  starting 2026-08-30 omits 2026-08-29). Descriptions that mention io_uring: a lower bound on kernel CVEs touching it, not a count of
  exploitable bugs. Rate: 58 in 321 days $= 0.181$ per day, 1.65 times the 2025 pace ($40/365 = 0.110$ per day).
- Controls: sysctl `kernel.io_uring_disabled` = 0 (all processes), 1 (only `CAP_SYS_ADMIN` or members of `io_uring_group`), 2 (none); rings
  already created keep working [3]. `io_uring/bpf_filter.c` exists in torvalds/linux master (`gh api repos/torvalds/linux/contents/io_uring`);
  its release and semantics are **unverified**. 113 commits touched `io_uring/` since 2026-10-01 (`gh api 'repos/torvalds/linux/commits?path=io_uring&since=2026-10-01T00:00:00Z'`).
- libuv 1.53.0 (2026-09-24) uses io_uring for file I/O on Linux; open issues #5408 and #5409 (2026-11-04) report a lost SQPOLL wakeup that leaves a
  request pending [4].

Decision rule: use io_uring only behind a runtime probe and the sysctl state, keep an epoll fallback, and never expose it inside a sandbox that
forbids it. Bun (`src/`, 2026-11-17 checkout) references io_uring in one file (`install/PackageInstall.rs`); whether its event loop uses it
on Linux is **unverified**: measure with `strace -c -f` before assuming.

## 2. sched_ext: schedulers as BPF programs

`kernel/sched/ext.c` exists at tag v6.12 and not at v6.11, so it merged in 6.12 (`gh api repos/torvalds/linux/contents/kernel/sched?ref=v6.12`). The
documentation requires `CONFIG_SCHED_CLASS_EXT=y` with BPF, JIT and BTF [5]. In master the code is split into `kernel/sched/ext/{ext,idle,arena,cid,sub,...}`.
sched-ext/scx: v1.1.4 on 2026-10-15, 105 open issues, Rust schedulers `scx_lavd`, `scx_rusty`, `scx_bpfland`, `scx_layered`, `scx_flash`, `scx_cosmos`
and others [6]. Relevance: a scheduler policy can be changed without rebooting, so the work-stealing and latency questions of `async.md`
section 4 reappear at kernel level (a runtime's $c_{eff}$ depends on what the kernel scheduler gives it); a custom policy must be benchmarked with
the open-loop method before it is trusted (`perf-math` skill).

## 3. Rust for Linux

Commit `9fa7153c31a3` (authored 2025-12-13, committed 2026-01-19, in Linux 7.0), "rust: conclude the Rust experiment", records the 2025 Maintainers Summit decision and states Rust "is here to
stay"; it removes the experimental notice from `Documentation/rust/index.rst` [7]. Latest kernels on 2026-11-17: mainline 7.4-rc3, stable 7.3.4, longterm
6.18.63, 6.12.117, 6.6.162, 6.1.192 [8]. `Documentation/process/changes.rst` lists Rust 1.85.0 and bindgen 0.71.1 as the (optional) minimum.
In-tree Rust: `rust/kernel` (110 entries), `drivers/gpu/nova-core`, `drivers/gpu/drm/nova`, `drivers/gpu/drm/tyr`, `drivers/android/binder` (Rust Binder),
`drivers/block/rnull` (`gh api repos/torvalds/linux/contents/<path>`, counts at 2026-11-17). Not verified here: per-architecture coverage and gccrs status.
The language blockers that matter to kernel Rust (specialization-free traits, allocator and fallible-allocation APIs, `Pin` ergonomics) overlap `compilers-languages.md`.

## 4. Windows NT I/O

IOCP is the stable completion model (libuv on Windows, tokio via mio/AFD) [9]. The I/O ring API (`ioringapi.h`) requires Windows 11 (build 22000) [10]:
operations build submission entries (`BuildIoRingReadFile`, register buffers and file handles) and submit them in a batch, the same amortisation as
io_uring ($o/B$). SDK 10.0.28000.0 additionally declares `BuildIoRingWriteFile`, `BuildIoRingFlushFile`, `BuildIoRingReadFileScatter` and
`BuildIoRingWriteFileGather`, which the Learn index did not list: documentation lags the SDK. Limits: file I/O only (no socket operations in the
header), thin documentation, per-operation availability must be probed with `IsIoRingOpSupported` / `QueryIoRingCapabilities`. libuv has no
IoRing use (`gh api search/code` for `IoRing repo:libuv/libuv` matched only the Linux sources and the ChangeLog); the Bun tree has none either.

## 5. kqueue and macOS

kqueue is a readiness interface (Lemon 2001 [11]); libuv has `src/unix/kqueue.c`, Bun a `KEventWatcher` for file watching. Apple's current
documentation URL returned 404 on 2026-11-17, so 2026 limits are **unverified**.

## 6. What this means for design

| Platform | Preferred model | Fallback | Blocker |
|---|---|---|---|
| Linux server | epoll everywhere; io_uring where the probe and policy allow | epoll | attack surface; sandbox policy; lost-wakeup class of bugs |
| Windows | IOCP | thread pool for blocking calls | IoRing is Windows 11 and file-only |
| macOS, BSD | kqueue | none | readiness only: disk I/O needs a thread pool |

## References
1. Google Security Blog, "Learnings from kCTF VRP's 42 Linux kernel exploits submissions", 2023-06: https://security.googleblog.com/2023/06/learnings-from-kctf-vrps-42-linux.html
2. NVD CVE API 2.0: https://nvd.nist.gov/developers/vulnerabilities
3. Linux `Documentation/admin-guide/sysctl/kernel.rst`, `io_uring_disabled`: https://docs.kernel.org/admin-guide/sysctl/kernel.html
4. libuv issues #5408 and #5409: https://github.com/libuv/libuv/issues/5408 and https://github.com/libuv/libuv/issues/5409
5. sched_ext documentation: https://docs.kernel.org/scheduler/sched-ext.html
6. sched-ext/scx releases: https://github.com/sched-ext/scx/releases
7. Linux commit 9fa7153c31a3: https://github.com/torvalds/linux/commit/9fa7153c31a3
8. kernel.org release list: https://www.kernel.org/releases.json
9. I/O completion ports: https://learn.microsoft.com/en-us/windows/win32/fileio/i-o-completion-ports
10. ioringapi.h: https://learn.microsoft.com/en-us/windows/win32/api/ioringapi/
11. Lemon, "Kqueue", 2001: https://people.freebsd.org/~jlemon/papers/kqueue.pdf
