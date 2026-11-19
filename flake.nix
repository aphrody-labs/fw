{
  description = "Fairwind v4.2 — Reproducible Full-Stack Pure-Rust Userland & 3-VPS Build Cluster Nix Flake (fr1, fr2, prd)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    # Keep upstream nixpkgs pin intact for 100% binary cache hits (Clang+ThinLTO CachyOS kernels)
    nix-cachyos-kernel.url = "github:xddxdd/nix-cachyos-kernel/release";
    nixos-wsl = {
      url = "github:nix-community/NixOS-WSL/main";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ flake-parts, nixpkgs, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];

      perSystem = { pkgs, system, ... }:
        let
          craneLib = inputs.crane.mkLib pkgs;
          src = craneLib.cleanCargoSource ./.;

          fairwindNative = craneLib.buildPackage {
            inherit src;
            pname = "fairwind-native";
            version = "4.2.0";
            cargoExtraArgs = "-p fw-bin";
            doCheck = true;
          };

          # Identical pure-Rust full-stack userland closure deployed to fr1, fr2, and prd
          userlandPackages = with pkgs; [
            fairwindNative
            # Pure-Rust POSIX coreutils (replaces GNU coreutils)
            uutils-coreutils-noprefix
            uutils-findutils
            uutils-diffutils
            # Modern Nov 2026 Rust CLIs (replaces bash/pwsh text pipelines)
            nushell
            ripgrep
            fd
            ast-grep
            jaq
            xh
            sd
            bat
            eza
            yazi
            jujutsu
            difftastic
            hyperfine
            tokei
            procs
            zoxide
            starship
            just
            mise
            uv
            # Distributed Rust build cluster toolchain
            sccache
            mold
            bun
          ];

          rustUserland = pkgs.buildEnv {
            name = "fairwind-rust-userland-4.2.0";
            paths = userlandPackages;
            pathsToLink = [ "/bin" "/share" ];
          };
        in {
          packages = {
            default = rustUserland;
            rust-userland = rustUserland;
            fairwind-native = fairwindNative;
          };

          devShells.default = pkgs.mkShell {
            packages = userlandPackages ++ (with pkgs; [
              rustc
              cargo
              clippy
              rustfmt
              clang
              lld
            ]);
            RUSTC_WRAPPER = "sccache";
            CARGO_INCREMENTAL = "0";
            CARGO_PROFILE_DEV_DEBUG = "line-tables-only";
          };
        };

      flake = {
        nixosModules = {
          fairwind-vps-common = import ./nix/modules/vps-common.nix;
          fairwind-os = import ./os/fairwind-os/flake.nix;
        };

        nixosConfigurations = let
          mkVps = hostModule: nixpkgs.lib.nixosSystem {
            system = "x86_64-linux";
            specialArgs = { inherit inputs; };
            modules = [
              inputs.self.nixosModules.fairwind-vps-common
              hostModule
            ];
          };
        in {
          fr1 = mkVps ./nix/hosts/fr1.nix;
          fr2 = mkVps ./nix/hosts/fr2.nix;
          prd = mkVps ./nix/hosts/prd.nix;
          wsl-fairwind = nixpkgs.lib.nixosSystem {
            system = "x86_64-linux";
            specialArgs = { inherit inputs; };
            modules = [
              inputs.nixos-wsl.nixosModules.default
              inputs.self.nixosModules.fairwind-vps-common
              ./nix/hosts/wsl-fairwind.nix
            ];
          };
        };
      };
    };
}
