{
  description = "Fairwind v4.1 — Fullstack Pure-Rust OS, Hybrid Cluster, WebAssembly/WebGPU, Hermes/OpenClaw Agent & NixOS Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];

      perSystem = { pkgs, system, ... }:
        let
          craneLib = inputs.crane.mkLib pkgs;
          src = craneLib.cleanCargoSource ./.;
          fairwindNative = craneLib.buildPackage {
            inherit src;
            pname = "fairwind-native";
            version = "4.1.0";
            cargoExtraArgs = "-p fw-bin";
            doCheck = true;
          };
        in {
          packages = {
            default = fairwindNative;
            fairwind-native = fairwindNative;
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              rustc
              cargo
              clippy
              rustfmt
              just
              nushell
              ripgrep
              fd
              ast-grep
              jaq
              xh
              sd
              bat
              eza
              jujutsu
              difftastic
              hyperfine
              bun
              uv
            ];
          };
        };

      flake.nixosModules.fairwind = import ./os/fairwind-os/flake.nix;
    };
}
