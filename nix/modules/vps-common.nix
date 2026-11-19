{ config, lib, pkgs, inputs, ... }:

let
  system = pkgs.stdenv.hostPlatform.system;
  fairwindUserland = inputs.self.packages.${system}.rust-userland;
  fairwindBin = inputs.self.packages.${system}.fairwind-native;
in {
  # Nix daemon reproducibility & store deduplication
  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    auto-optimise-store = true;
    max-jobs = "auto";
    cores = 0;
  };

  # Identical full-stack pure-Rust userland on every VPS
  environment.systemPackages = [
    fairwindUserland
    pkgs.clang
    pkgs.lld
    pkgs.xfsprogs
  ];

  environment.variables = {
    RUSTC_WRAPPER = "sccache";
    CARGO_INCREMENTAL = "0";
    CARGO_PROFILE_DEV_DEBUG = "line-tables-only";
  };

  # ZRAM zstd swap (prio 100, ZSWAP disabled to avoid double compression)
  zramSwap = {
    enable = true;
    algorithm = "zstd";
    priority = 100;
    memoryPercent = 25;
  };

  # Verified Nov 2026 kernel VM, THP, and TCP BBR tuning for build clusters
  boot.kernel.sysctl = {
    "vm.watermark_boost_factor" = 0;
    "vm.watermark_scale_factor" = 125;
    "vm.page-cluster" = 0;
    "vm.vfs_cache_pressure" = 50;
    "kernel.sched_autogroup_enabled" = 0;
    "net.core.default_qdisc" = "fq";
    "net.ipv4.tcp_congestion_control" = "bbr";
  };

  systemd.tmpfiles.rules = [
    "w /sys/kernel/mm/transparent_hugepage/enabled - - - - madvise"
  ];

  # Block I/O scheduler: none on ROTA=0 virtual SSDs (never bfq)
  services.udev.extraRules = ''
    ACTION=="add|change", KERNEL=="sd[a-z]", ATTR{queue/rotational}=="0", ATTR{queue/scheduler}="none"
  '';

  # Unified Fairwind control-plane + MCP/GraphQL/WebRTC/GGML-RPC service
  systemd.services.fairwind-native = {
    description = "Fairwind v4.2 Full-Stack Rust Control Plane & Hermes/OpenClaw Agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "${fairwindBin}/bin/fairwind-native serve 127.0.0.1:4380";
      Restart = "always";
      RestartSec = 2;
      NoNewPrivileges = true;
      ProtectSystem = "strict";
      PrivateTmp = true;
    };
  };
}
