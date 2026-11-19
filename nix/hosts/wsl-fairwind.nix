{ inputs, pkgs, lib, ... }:

# Fairwind-WSL — Fused NixOS-WSL (nix-community/NixOS-WSL 2605.7.2) + CachyOS-WSL (2026.11.05.5)
# Tuned for Windows 11 WSL 3.0.5 + NVIDIA RTX 4070 (dxgkrnl CUDA/Vulkan/WebGPU) + 24 vCPU / 32 GB RAM
{
  networking.hostName = "fairwind-wsl";

  # NixOS-WSL integration (when imported with nixos-wsl.nixosModules.default)
  wsl = {
    enable = true;
    defaultUser = "aphro";
    startMenuLaunchers = true;
    useWindowsDriver = true; # exposes /usr/lib/wsl/lib (libcuda.so, libdxcore.so) for RTX 4070
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
        appendWindowsPath = false; # keep PATH pure-Rust & fast (<15ms startup)
      };
    };
  };

  # CachyOS-grade environment + RTX 4070 CUDA/Vulkan/WebGPU + Wayland/WSLg tuning
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

  # Hardware GPU / Vulkan / OpenGL acceleration inside WSL2
  hardware.graphics = {
    enable = true;
    extraPackages = with pkgs; [ mesa vulkan-loader vulkan-tools ];
  };

  # Fuse CachyOS-WSL ergonomics (fish/nu/pacman-compat tools) with Fairwind pure-Rust userland
  environment.systemPackages = with pkgs; [
    inputs.self.packages.x86_64-linux.rust-userland
    wslu
    wl-clipboard
    xfsprogs
    btrfs-progs
    vulkan-tools
    clang
    lld
  ];
}
