# ADR: Long-Term OS Architecture (`fw` v4.2, 2026–2030)

- **Date**: 2026-11-19
- **Status**: Accepted and enforced across `flake.nix`, `nix/`, `os/`, and `upstreams.toml`
- **Hard Requirement**: Full Linux kernel performance (bare-metal, VPS, WSL, GPU, `io_uring`, `sched_ext`, `eBPF`/XDP) with maximum Rust memory safety.

## Decision

Adopt a **2-tier OS architecture**:

1. **Tier 1 — Primary Host OS (90% investment): Linux CachyOS LTO + Pure-Rust Userland via NixOS**
   - **Kernel**: `xddxdd/nix-cachyos-kernel` (`lto-x86_64-v3` on `fr1`/`fr2`, `lto-zen4` on `prd`) and custom Linux 6.18+ BORE/`sched_ext`/ThinLTO/`virtiofs`/`dxgkrnl` kernel on Windows WSL ([os/fairwind-wsl](../../os/fairwind-wsl/)).
   - **Scheduler**: `sched-ext/scx` (`v1.1.4`, `scx_lavd` Rust BPF scheduler enabled via `services.scx` in [nix/modules/vps-common.nix](../../nix/modules/vps-common.nix)).
   - **Kernel Tuning**: `kernel.io_uring_disabled = 0`, `net.ipv4.tcp_congestion_control = bbr`, `net.core.default_qdisc = fq`, `net.ipv4.tcp_fastopen = 3`, `vm.max_map_count = 1048576`, `fs.file-max = 2097152`, `zswap.enabled=0` + ZRAM `zstd`, `transparent_hugepage=madvise`, `queue/scheduler=none` on SSD/NVMe.
   - **Userland**: 100% pure-Rust core layer (`fw`, `uutils/coreutils` `0.12.0`, `sudo-rs` `v0.2.15`, `ntpd-rs` `v1.9.0`, `fw-wayland-wgpu`, `virtiofsd`) declared in [flake.nix](../../flake.nix).

2. **Tier 2 — Agent & Untrusted Sandbox MicroVMs (10% investment)**
   - **Production Sandbox VMM**: `cloud-hypervisor/cloud-hypervisor` (`v54.1`) on Linux hosts and `microsoft/openvmm` (`v0.5.0`) on Windows/WSL, running minimal Linux guests over `virtio-fs` + `vsock`.
   - **Optional Memory-Safe Guest R&D**: `asterinas/asterinas` (`v0.19.0`, pre-1.0) retained strictly in [os/fairwind-os](../../os/fairwind-os/) as an optional microVM guest. Asterinas is not a primary host OS because it lacks bare-metal GPU DRM/CUDA/Vulkan drivers, `io_uring` zero-copy parity, `sched_ext`, and `eBPF`/XDP.

## Verified Upstream Baselines (2026-11-19)

| Upstream | Release | Stars | Role in `fw` |
| :--- | :--- | ---: | :--- |
| `xddxdd/nix-cachyos-kernel` | rolling | 796 | Tier-1 LTO CachyOS kernels (`x86_64-v3`, `zen4`) |
| `sched-ext/scx` | `v1.1.4` | 2,253 | Rust BPF schedulers (`scx_lavd`, `scx_bpfland`) |
| `cloud-hypervisor/cloud-hypervisor` | `v54.1` | 6,541 | Tier-2 Linux KVM microVM sandbox VMM |
| `microsoft/openvmm` | `v0.5.0` | 1,976 | Tier-2 Windows/WSL Rust VMM |
| `uutils/coreutils` | `0.12.0` | 24,393 | Pure-Rust POSIX coreutils (`uutils-coreutils-noprefix`) |
| `trifectatechfoundation/sudo-rs` | `v0.2.15` | 4,518 | Memory-safe privilege escalation |
| `pendulum-project/ntpd-rs` | `v1.9.0` | 1,159 | Memory-safe NTP/NTS daemon |
| `asterinas/asterinas` | `v0.19.0` | 5,116 | Optional Tier-2 guest microVM (non-primary) |
