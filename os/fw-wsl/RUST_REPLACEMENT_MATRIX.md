# Fw Zero-Legacy WSL Replacement Architecture (2026-11-19)

Complete 1-to-1 replacement of every legacy C/C++ component in `microsoft/WSL`, `microsoft/wslg`, and `microsoft/WSL2-Linux-Kernel` with verified November 2026 experimental Rust replacements and CachyOS kernel patches.

| Legacy WSL / WSLg Component (C/C++) | Removed / Replaced By (Rust / CachyOS) | Verified Repo & Status (`2026-11-19`) | Why |
|---|---|---|---|
| `wslservice.exe` / Hyper-V HCS C++ VMM | **OpenVMM / OpenHCL** or **Cloud-Hypervisor** | `microsoft/openvmm` (1,976★, Rust), `cloud-hypervisor/cloud-hypervisor` (6,540★, Rust) | Memory-safe Rust VMM supporting WHP/KVM, virtio-fs, virtio-gpu, virtio-vsock |
| Plan 9 `9p` C filesystem server / `drvfs` | **`virtiofsd`** (`CONFIG_VIRTIO_FS=y`, `# CONFIG_9P_FS is not set`) | `gitlab.com/virtio-fs/virtiofsd` (Rust) | 10–30× faster host↔guest filesystem I/O via DAX/virtio-fs |
| WSLg **Weston** (C) + **XWayland** | **`fw-wayland-wgpu`** + **Smithay** / **COSMIC Comp** | `Smithay/smithay` (3,396★, Rust), `pop-os/cosmic-comp` (842★, Rust), `crates/fw-wayland-wgpu` | Zero `libwayland-client.so` C deps; native Vulkan/WebGPU/GPUI rendering |
| WSLg **FreeRDP** (C) | **IronRDP** | `Devolutions/IronRDP` (3,316★, Rust) | 100% memory-safe Rust RDP transport |
| WSLg **PulseAudio** (C) | **PipeWire** + Rust `cpal` | `services.pipewire.enable = true` | Low-latency graph audio replacing legacy PulseAudio |
| `wslrelay` / `gvisor-tap-vsock` (C++/Go) | **`smoltcp`** + **`boringtun`** + `CONFIG_VIRTIO_VSOCKETS=y` | `smoltcp-rs/smoltcp` (4,667★, Rust), `cloudflare/boringtun` (7,271★, Rust) | Zero-copy Rust TCP/IP + WireGuard + native vsock |
| Stock WSL2 CFS/EEVDF kernel | **CachyOS BORE + `sched_ext` (`scx_lavd` / `scx_rustland`) + Clang ThinLTO** | `CachyOS/kernel-patches`, `sched-ext/scx` (2,253★, Rust schedulers), `CONFIG_DXGKRNL=y` | Interactive burst latency + RTX 4070 CUDA/Vulkan/WebGPU preserved |
| Legacy `/init`, `wslu`, GNU coreutils, `sudo`, `timesyncd` | **`fw`** + **`uutils`** + **`sudo-rs`** + **`ntpd-rs`** + **`nushell`** | `crates/fw-{uutils,hermes-claw,ostd-wasm}`, `uutils/coreutils` (24,391★), `trifectatechfoundation/sudo-rs` (4,518★) | 100% pure-Rust userland identical across Windows WSL + `fr1`/`prd` |
| Isolated Linux Guest Kernel (future target) | **Asterinas OSDK** (`os/fw-os/`) | `asterinas/asterinas` (5,116★, Rust framekernel) | Linux ELF ABI-compatible `#![no_std] #![deny(unsafe_code)]` Rust kernel |
