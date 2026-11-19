{ inputs, pkgs, lib, ... }:

# Fw-WSL Zero-Legacy Pure-Rust Edition
# Strips legacy WSL shell/C wrappers (wslu, 9p, PulseAudio, Weston/FreeRDP, GNU sudo/coreutils)
# Replaces with: OpenVMM/virtiofsd + CachyOS BORE/scx_lavd + fw-wayland-wgpu/Smithay + IronRDP + rust-userland
{
  networking.hostName = "fw-wsl";

  wsl = {
    enable = true;
    defaultUser = "fw";
    startMenuLaunchers = false; # replaced by native Fw launcher
    useWindowsDriver = true;    # RTX 4070 dxgkrnl CUDA/Vulkan/WebGPU (/usr/lib/wsl/lib)
    interop.register = true;
    wslConf = {
      automount = {
        enabled = true;
        root = "/mnt";
        options = "metadata,umask=22,fmask=11";
      };
      boot.systemd = true;
      network.generateResolvConf = true;
      interop = {
        enabled = true;
        appendWindowsPath = false; # zero Windows PATH pollution; pure-Rust PATH only
      };
    };
  };

  # Pure-Rust / CachyOS experimental runtime environment (NVIDIA RTX 4070 Vulkan/WebGPU + Wayland)
  environment.variables = {
    CUDA_PATH = "/usr/lib/wsl/lib";
    MESA_D3D12_DEFAULT_ADAPTER_NAME = "NVIDIA";
    GALLIUM_DRIVER = "d3d12";
    WGPU_BACKEND = "vulkan";
    GDK_BACKEND = "wayland";
    QT_QPA_PLATFORM = "wayland";
    WINIT_UNIX_BACKEND = "wayland";
    RUSTC_WRAPPER = "sccache";
    CARGO_INCREMENTAL = "0";
    CARGO_PROFILE_DEV_DEBUG = "line-tables-only";
    RUSTFLAGS = "-C target-cpu=native -C link-arg=-fuse-ld=mold";
  };

  # Modern PipeWire audio (replaces legacy WSLg PulseAudio C daemon)
  services.pipewire = {
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
  };

  # Hardware GPU / Vulkan acceleration
  hardware.graphics = {
    enable = true;
    extraPackages = with pkgs; [ mesa vulkan-loader vulkan-tools ];
  };

  # Zero legacy wslu/GNU bloat: 100% pure-Rust userland + virtiofsd + sched_ext Rust schedulers
  environment.systemPackages = with pkgs; [
    inputs.self.packages.x86_64-linux.rust-userland
    virtiofsd
    scx.rustscheds
    wl-clipboard
    xfsprogs
    btrfs-progs
    vulkan-tools
    clang
    lld
  ];
}
