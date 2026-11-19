{ inputs, ... }:

# fr1 (vps-203bea89) — Primary 12 vCPU Intel Haswell (x86_64-v3) / 47 GB RAM Heavy Build & NativeLink/sccache Scheduler
{
  networking.hostName = "fr1";
  boot.kernelPackages = inputs.nix-cachyos-kernel.legacyPackages.x86_64-linux.linuxPackages-cachyos-latest-lto-x86_64-v3;

  fileSystems."/srv/xbuild" = {
    device = "/dev/mapper/xbuild-data";
    fsType = "xfs";
    options = [ "noatime" "inode64" "logbufs=8" "logbsize=256k" "noquota" "nofail" ];
  };

  boot.kernel.sysctl = {
    "vm.swappiness" = 100;
    "vm.dirty_bytes" = 1073741824;
    "vm.dirty_background_bytes" = 268435456;
  };
}
