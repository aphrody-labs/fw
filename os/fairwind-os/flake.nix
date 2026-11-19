{
  description = "Fairwind v4.0 — 100% Pure-Rust OS (Asterinas OSDK + uutils multicall + WebAssembly/WebGPU + NixOS compatible)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in {
      packages.${system} = {
        fairwind-native = pkgs.rustPlatform.buildRustPackage {
          pname = "fairwind-native";
          version = "4.0.0";
          src = ../..;
          cargoLock.lockFile = ../../Cargo.lock;
          cargoBuildFlags = [ "-p" "fw-bin" ];
        };

        uutils-rootfs = pkgs.runCommand "fairwind-uutils-rootfs" {} ''
          mkdir -p $out/bin $out/nix/store $out/etc
          ln -s ${pkgs.uutils-coreutils-noprefix}/bin/coreutils $out/bin/uutils
          for cmd in ls cat echo cp mv rm mkdir wc head tail sha256sum uname env pwd stat; do
            ln -s /bin/uutils $out/bin/$cmd
          done
        '';
      };

      # Drop-in NixOS module replacing GNU coreutils with uutils-coreutils-noprefix (wiki.nixos.org/wiki/Uutils)
      nixosModules.fairwind = { config, lib, pkgs, ... }: {
        environment.systemPackages = with pkgs; [
          uutils-coreutils-noprefix
          uutils-findutils
          uutils-diffutils
          self.packages.${system}.fairwind-native
        ];

        systemd.services.fairwind-cluster = {
          description = "Fairwind v4.0 Zero-Dep Hybrid Cluster (Pingora + Dynamo + ggml-rpc + uutils)";
          wantedBy = [ "multi-user.target" ];
          serviceConfig = {
            ExecStart = "${self.packages.${system}.fairwind-native}/bin/fairwind-native serve 0.0.0.0:8080";
            Restart = "always";
          };
        };
      };
    };
}
