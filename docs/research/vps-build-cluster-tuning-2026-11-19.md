# 3-VPS Unified Build Cluster Tuning: Swap, RAM, CPU, Disk, Kernel & XFS (2026-11-19)

Dated deep-research blueprint grounded in **live read-only telemetry** from `fr1`, `fr2`, and `prd` plus **verified November 2026 upstream sources**.

## 1. Live Fleet Baseline (`2026-11-19`)

| Host | Kernel | CPU | RAM (total / avail) | Swap / ZRAM | Storage & Mounts | Scheduler |
|---|---|---|---|---|---|---|
| `fr1` (`vps-203bea89`) | `7.3.0-10-generic` | 12 vCPU Intel Haswell | 47 GB / 33 GB | `zram0` 12G `zstd` (10.1G/3.1G, prio 100) + `/swapfile2` 8G (prio 10) | `/` ext4 290G (**90%**), `/srv/xbuild` XFS 805G (**100% — 7.4G free**) | `sda=none`, **`sdb=bfq`**, **`sdc=bfq`** |
| `fr2` (`bunos`) | `7.3.0-10-generic` | 6 vCPU Intel Haswell | 11.7 GB / 8.0 GB | `zram0` 5.7G `zstd` (3.4G/2.0G, prio 100) + `/swapfile` 2G (prio 10) | `/` ext4 96G (**98% — 2.2G free**), `/srv/xbuild` XFS 4G (16%), NFSv4.2 `10.200.77.1:/srv/xbuild/shared` (`nconnect=8`) | `sda=none` |
| `prd` (`srv2116886`) | `7.4.0-rc3-1-cachyos-rc` | 8 vCPU **AMD EPYC 9354P** (Zen 4) | 32 GB / 29 GB | `zram0` 7.8G `zstd` (4K used, prio 100) + `/swap/swapfile` 8G (prio 10) | `/` btrfs 300G (24%, `zstd:1`, 223G free), `/srv/nie/steam` XFS 100G (62%, `logbsize=256k,largeio`) | `sda=none` |

All 3 hosts already have: `MGLRU=0x0007`, `ZSWAP=N`, `vm.page-cluster=0`, `kernel.sched_autogroup_enabled=0`, `fq` + `bbr`, `sccache`, `mold`, `lld`, `clang`, `cargo`, `rustc`, `zramctl`, `xfs_info` (`wild` on `fr1`/`fr2`).

## 2. Critical Bottlenecks Found Live

1. **Disk saturation (P0)**:
   - `fr1:/srv/xbuild` is **100% full (798G / 805G)** — XFS allocator performance collapses above 90–95% utilization.
   - `fr2:/` is **98% full (94G / 96G)** — imminent ENOSPC during builds/tests.
   - Fix: prune stale `CARGO_TARGET_DIR` / `nativelink` CAS (`CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`) to keep XFS `<= 80%` used.
2. **Block I/O scheduler on `fr1` (`sdb`/`sdc`)**:
   - `sdb` (500G) and `sdc` (305G) backing LVM `/dev/mapper/xbuild-data` are `ROTA=0` virtual SSDs running `bfq` instead of `none`.
   - Fix (udev rule): `ACTION=="add|change", KERNEL=="sd[b-c]", ATTR{queue/rotational}=="0", ATTR{queue/scheduler}="none"`.
3. **XFS mount options on `fr1` & `fr2`**:
   - Currently `logbsize=32k`; upgrade to **`logbsize=256k,logbufs=8,noatime,inode64,noquota`** (matching `prd:/srv/nie/steam`) to cut journal flush stalls during parallel `rustc`/`lld`/`nativelink` writes.
   - All XFS volumes already have `reflink=1,crc=1,finobt=1,rmapbt=1,nrext64=1` — use `cp --reflink=auto` for O(1) zero-copy build workspaces.
4. **RAM, Swap, THP & VM sysctls**:
   - Keep `zram0` (`zstd`) at priority 100 + disk swapfile at priority 10 + `ZSWAP=N` (never double-compress) + `vm.page-cluster=0` + `MGLRU=0x0007`.
   - On `fr1`, switch THP from `[always]` to **`madvise`** (`/sys/kernel/mm/transparent_hugepage/enabled`) to prevent synchronous compaction stalls during linker bursts.
   - On `fr1` & `fr2`, set `vm.watermark_boost_factor=0` (matching `prd`) and `vm.watermark_scale_factor=125` to eliminate premature `kswapd` thrashing that pushed `10.1G` into `fr1`'s `zram0` despite 33 GB free RAM.
   - Keep `systemd-oomd` / cgroup v2 `MemoryHigh` limits on build slices so OOM kills only runaway `rustc`/`clang` jobs, never SSH or prod services.
5. **CPU & Unified 3-VPS Build Cluster Architecture**:
   - **Primary builder (`fr1`, 12c/47G)**: hosts `nativelink` CAS/scheduler + `sccache-dist` scheduler + NFSv4.2 shared cache (`10.200.77.1`, `nconnect=8`).
   - **Secondary worker & test/lint host (`fr2`, 6c/11.7G)**: runs `sccache-dist` / `nativelink` worker + test/lint gates (`CARGO_BUILD_JOBS=4`, `MemoryHigh=8G`).
   - **Production + cgroup-isolated burst compile worker (`prd`, 8c AMD EPYC 9354P / 32G)**: Zen 4 IPC is ~2.5× Haswell per core; isolate any remote `sccache-dist` / `nativelink` worker inside a strict systemd slice (`Nice=19`, `IOSchedulingClass=idle`, `CPUWeight=15`, `MemoryHigh=12G`, `MemoryMax=16G`) so production latency is never affected.

## 3. Verified Nov 2026 Build Cluster Libraries & Tools

| Role | Project | Stars | Verified HEAD (`2026-11-19`) | Use in 3-VPS Cluster |
|---|---|---:|---|---|
| Distributed Compiler Cache | `mozilla/sccache` | 7,951 | `ef8e7d76f9` (2026-11-17) | `RUSTC_WRAPPER=sccache` + `sccache-dist` over WireGuard `10.200.77.x` + R2/XFS cache |
| Fast Release Linker | `rui314/mold` | 17,762 | `331ac66366` (2026-10-22) | `-C link-arg=-fuse-ld=mold` for release / Fat-LTO builds |
| Fast Incremental Rust Linker | `davidlattimore/wild` | 4,125 | `d52361a23b` (2026-11-19) | `-C link-arg=-fuse-ld=wild` for dev/test incremental loops |
| Rust RBE & CAS Cluster | `TraceMachina/nativelink` | 1,649 | `ac7db3c15c` (2026-11-19) | Already present on `fr1` (`/var/lib/private/nativelink`); unifies CAS + RBE across all 3 VPS |
| Polyglot Build Graph | `facebook/buck2` | 4,534 | `afc24f3b96` (2026-11-19) | Optional RBE frontend talking directly to NativeLink |
| Kernel ZRAM Provisioning | `systemd/zram-generator` | 815 | `7855941d1a` (2025-05-25) | Declarative `zstd` ZRAM swap sizing (`min(ram / 2, 16G)`) |

## 4. Recommended Cargo / Rustc Cluster Env

```toml
# Shared across fr1 / fr2 / prd build workers
[build]
rustc-wrapper = "sccache"

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold", "-C", "symbol-mangling-version=v0"]

[profile.dev]
debug = "line-tables-only"
incremental = false
```
