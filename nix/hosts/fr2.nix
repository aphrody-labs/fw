{ inputs, ... }:

# fr2 (bunos) — Secondary 6 vCPU Intel Haswell (x86_64-v3) / 11.7 GB RAM Build Worker, Test/Lint & NFSv4.2 Client
{
  networking.hostName = "fr2";
  boot.kernelPackages = inputs.nix-cachyos-kernel.legacyPackages.x86_64-linux.linuxPackages-cachyos-latest-lto-x86_64-v3;

  fileSystems."/srv/xbuild/shared" = {
    device = "10.200.77.1:/srv/xbuild/shared";
    fsType = "nfs4";
    options = [
      "vers=4.2"
      "rsize=1048576"
      "wsize=1048576"
      "nconnect=8"
      "noatime"
      "_netdev"
      "nofail"
      "x-systemd.automount"
      "x-systemd.idle-timeout=600"
    ];
  };

  boot.kernel.sysctl = {
    "vm.swappiness" = 150;
    "vm.dirty_bytes" = 268435456;
    "vm.dirty_background_bytes" = 67108864;
  };
}
