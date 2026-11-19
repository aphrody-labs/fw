{ ... }:

# fr1 (vps-203bea89) — Primary 12 vCPU / 47 GB RAM Heavy Build & NativeLink/sccache Scheduler
{
  networking.hostName = "fr1";

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
