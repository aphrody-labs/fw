# mold audit (2026-11-21)

Sources were read live today: `gh api repos/rui314/mold`, its releases, and search/issues on the repo. Linker versions were read with `--version` on our two nodes, and the cargo configs from fw main.

## State of the project

- **Repository:**
  - rui314/mold: 17,767 stars, 591 forks, MIT, default branch `main`.
  - Last commit `331ac66366` on 2026-10-22.
  - 2,898 commits in the last 13 weeks of GitHub's commit activity, the weeks of the rewrite.
- **Releases:**

  | Version | Date |
  |---|---|
  | **v3.0.0** | 2026-10-05 |
  | v2.42.1 | 2026-09-11 |
  | v2.42.0 | 2026-08-12 |
  | v2.41.0 | 2026-04-13 |
  | v2.40.4 | 2025-08-17 |

- **mold 3.0 is a rewrite from C++ to Rust,** announced as a drop-in replacement for 2.42.1: same options, same targets, same output apart from listed fixes.
  - It builds with Cargo and needs Rust 1.95 or later plus a C compiler.
  - oneTBB is gone; mimalloc 3.5.3 is linked statically, or the system allocator with `--features system-allocator`.
  - Targets are now Cargo features, and the tests run with `cargo test`.
  - Malformed input now hits Rust bounds checks instead of out-of-bounds reads.
- **Issues and PRs:**
  - 196 open issues; 0 carry the `bug` label, so labels are not used for triage.
  - 34 open PRs.
  - Since 2026-08-21: 32 issues closed, 31 PRs merged.
  - Open issues mentioning: LTO 32, rust 30, windows 14, wasm 1.
- **Recent issues:** #1764 to #1777, all opened 2026-11-17, read like a fuzzing campaign against 3.0.
  - Bounds-check panics on out-of-range section-name offsets, relocation symbol indexes, x86-64, ARM, i386 and RISC-V relocation offsets, and unsorted `.eh_frame` relocations.
  - `-r` gaps: no build-id note, `SHF_GROUP` lost in COMDAT groups.
  - TLSLE relocations accepted in shared objects on AArch64 and RISC-V; LoongArch `e_flags` lost.

  A panic is safer than the old out-of-bounds read, but on hostile or corrupt objects it is still a crash.
- **crates.io:** #1661 "Release mold on crates.io" (opened 2026-09-19) is still open, so mold is not yet a crate we can depend on.
- **Recent open PRs:**
  - sysroot for symlinked scripts (#1778);
  - keep symbols of ICF-folded sections (#1763);
  - an LTO archive preemption fix for GCC (#1754);
  - a series of `-r`, symtab and `.gnu.attributes` fixes by orgads (#1746 to #1753).

## What we run

| Node | mold | wild |
|---|---|---|
| fr1 | 2.42.0, the C++ build | 0.11.0 |
| prd | 3.0.0 (8de38c35) | not installed |

The `wild` crate is wild-linker 0.11.0; the release is 0.11.0 of 2026-10-24.

The cargo configs on fw main name no ELF linker for Linux. Only `aphrody/.cargo/config.toml` sets `lld-link.exe` for Windows and `-fuse-ld=lld` for aarch64 Linux, because libwebrtc needs it.

The crate-layout lot measured dev relink CPU of fw-bin falling from 1.57 s to about 1.2 s with wild or mold.

mold and wild link ELF only. Windows (PE/COFF) stays on `lld-link`, and wasm on `rust-lld` / `wasm-ld`.

## Decisions

1. **Same linker on every node.** Bring every node to the same Rust linkers, as the unified cluster requires: mold 3.0.0 on fr1 (it still runs the C++ 2.42.0), and wild 0.11.0 on prd.
   - Build from source with Cargo on the node's own ISA, never by copying binaries from prd to fr1: prd is x86-64-v4, fr1 is x86-64-v3.
2. **Pick by measurement.** Choose between mold 3.0 and wild on our real links:
   - fw-bin release with fat LTO;
   - the aphrody `zed` binary;
   - dev relinks.

   Measure wall time and peak RSS, with three runs each. Set the winner for `x86_64-unknown-linux-gnu` and `-musl` in the workspace `.cargo/config.toml`, through `-fuse-ld` or the `linker` key per target. Both are Rust now, which fits the pure-Rust order.
3. **Keep `lld` where it is needed:** Windows (`lld-link`) and the libwebrtc aarch64 Linux link, until it is checked on mold 3.0 or wild.
4. **Follow upstream** in the fw upstream catalog: tag releases, issue #1661 (when mold reaches crates.io, it can become a pinned build dependency instead of a system binary), and the fuzzing issue series.
5. **Lot ownership:** this belongs to `docs/agents/queue/modern-toolchain-every-step.md` (build tooling, step 6) and the unified-cluster lot (node tools).
