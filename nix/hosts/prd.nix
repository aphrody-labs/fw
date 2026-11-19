{ ... }:

# prd (srv2116886) — Production 8 vCPU AMD EPYC 9354P (Zen 4) / 32 GB RAM + Cgroup-Isolated Burst Worker
{
  networking.hostName = "prd";

  boot.kernel.sysctl = {
    "vm.swappiness" = 150;
    "vm.dirty_bytes" = 536870912;
    "vm.dirty_background_bytes" = 134217728;
  };

  # Strict cgroup v2 isolation so burst builds never impact production services
  systemd.slices.fairwind-build-worker = {
    description = "Fairwind Cgroup-Isolated Burst Build Worker Slice (never impacts prod)";
    sliceConfig = {
      CPUWeight = 15;
      IOWeight = 15;
      MemoryHigh = "12G";
      MemoryMax = "16G";
    };
  };
}
