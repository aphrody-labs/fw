{ config, lib, pkgs, inputs, ... }:

let
  system = pkgs.stdenv.hostPlatform.system;
  fairwindUserland = inputs.self.packages.${system}.rust-userland;
  fwBin = inputs.self.packages.${system}.fw;
in {
  # Nix daemon reproducibility & store deduplication
  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    auto-optimise-store = true;
    max-jobs = "auto";
    cores = 0;
  };

  # Identical full-stack pure-Rust userland + Linux microVM sandbox tools on every host
  environment.systemPackages = [
    fairwindUserland
    pkgs.clang
    pkgs.lld
    pkgs.xfsprogs
    pkgs.scx.rustscheds
    pkgs.cloud-hypervisor
    pkgs.virtiofsd
  ];

  environment.variables = {
    RUSTC_WRAPPER = "sccache";
    CARGO_INCREMENTAL = "0";
    CARGO_PROFILE_DEV_DEBUG = "line-tables-only";
  };

  # Pure-Rust system daemons & privilege escalation (replaces GNU sudo & systemd-timesyncd)
  security.sudo.enable = false;
  security.sudo-rs.enable = true;
  services.timesyncd.enable = false;
  services.ntpd-rs.enable = true;

  # Rust BPF CPU scheduler (sched_ext) on Linux CachyOS LTO kernel
  services.scx = {
    enable = true;
    scheduler = "scx_lavd";
  };

  # ZRAM zstd swap (prio 100, ZSWAP disabled to avoid double compression)
  zramSwap = {
    enable = true;
    algorithm = "zstd";
    priority = 100;
    memoryPercent = 25;
  };

  boot.kernelParams = [
    "zswap.enabled=0"
    "transparent_hugepage=madvise"
    "nvme_core.default_ps_max_latency_us=0"
  ];

  # Verified Nov 2026 Linux kernel VM, io_uring, THP, and TCP BBR tuning for high-perf clusters
  boot.kernel.sysctl = {
    "vm.watermark_boost_factor" = 0;
    "vm.watermark_scale_factor" = 125;
    "vm.page-cluster" = 0;
    "vm.vfs_cache_pressure" = 50;
    "vm.max_map_count" = 1048576;
    "fs.file-max" = 2097152;
    "fs.inotify.max_user_watches" = 524288;
    "kernel.sched_autogroup_enabled" = 0;
    "kernel.io_uring_disabled" = 0;
    "net.core.default_qdisc" = "fq";
    "net.core.rmem_max" = 16777216;
    "net.core.wmem_max" = 16777216;
    "net.core.netdev_max_backlog" = 16384;
    "net.ipv4.tcp_congestion_control" = "bbr";
    "net.ipv4.tcp_fastopen" = 3;
    "net.ipv4.tcp_mtu_probing" = 1;
  };

  systemd.tmpfiles.rules = [
    "w /sys/kernel/mm/transparent_hugepage/enabled - - - - madvise"
  ];

  # Block I/O scheduler: none on ROTA=0 virtual SSDs & NVMe (never bfq)
  services.udev.extraRules = ''
    ACTION=="add|change", KERNEL=="sd[a-z]|nvme[0-9]n[0-9]", ATTR{queue/rotational}=="0", ATTR{queue/scheduler}="none"
  '';

  # Unified fw control-plane + MCP/GraphQL/WebRTC/GGML-RPC service
  systemd.services.fw = {
    description = "fw v4.2 Full-Stack Rust Control Plane & Hermes/OpenClaw Agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "${fwBin}/bin/fw serve 127.0.0.1:4380";
      Restart = "always";
      RestartSec = 2;
      NoNewPrivileges = true;
      ProtectSystem = "strict";
      PrivateTmp = true;
    };
  };
}
