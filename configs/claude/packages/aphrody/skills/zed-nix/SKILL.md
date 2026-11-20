---
name: zed-nix
description: >-
  Authoritative Nix architecture distilled from zed-industries/zed/nix (14 files + root flake.nix):
  flake-parts with dev partitions (keeping consumer flake.lock lean), crane + rust-overlay pinned to
  rust-toolchain.toml, overlays, devShells, treefmt-nix (nixfmt + rustfmt), and pkgs.testers.nixosTest
  KVM VM integration suites for GPUI AccessKit AT-SPI (a11y.nix) and Linux Bubblewrap sandboxing.
---

# Zed Nix Architecture (`zed-industries/zed/nix`)

Synced from upstream `zed-industries/zed` (`flake.nix` + `nix/`, 14 files mirrored in `../../docs/zed/nix/`).

## 1. Flake-Parts + Dev Partitions Pattern

- **Root `flake.nix`** (`../../docs/zed/nix/root-flake.nix`): Minimal consumer inputs only (`nixpkgs`, `flake-parts`, `crane`, `rust-overlay`).
- **Partitions** (`nix/modules/partitions.nix` + `nix/dev/flake.nix`):
  - Routes `devShells`, `formatter`, and `checks` into the `dev` partition (`extraInputsFlake = ../dev`).
  - Dev-only inputs (`treefmt-nix`, `corgi-src`, pinned `mdBook`) stay in `nix/dev/flake.lock` and never pollute downstream consumers' lockfiles.

## 2. Toolchain, Crane Build & Overlays

- **Single source of truth for Rust** (`nix/toolchain.nix`):
  - `rustBin = inputs.rust-overlay.lib.mkRustBin { } pkgs;`
  - `rustToolchain = rustBin.fromRustupToolchainFile ../rust-toolchain.toml;`
  - Passes `crane = inputs.crane.mkLib pkgs;`, `rustToolchain`, and `commitSha = inputs.self.rev or null;` into `nix/build.nix`.
- **Packages** (`nix/modules/packages.nix`):
  - `packages.default` = release build (`zed-editor`).
  - `packages.debug` = `zed-editor.override { profile = "dev"; }`.
- **Overlay** (`nix/modules/overlays.nix`): exposes `flake.overlays.default` providing `zed-editor`.
- **Formatter** (`nix/modules/treefmt.nix`): `treefmt-nix` enabling `programs.nixfmt` and `programs.rustfmt`.
- **DevShells** (`nix/modules/devshells.nix`): complete Linux/macOS native build dependencies + tooling shell without container overhead.

## 3. NixOS VM Integration Tests (`pkgs.testers.nixosTest`)

1. **GPUI AccessKit AT-SPI Test** (`nix/tests/a11y.nix` + `nix/tests/a11y_atspi_test.py`):
   - Boots a NixOS VM with X11 + AT-SPI2, launches `gpui-a11y-example`, walks the AT-SPI tree with `pyatspi`, and verifies accessible roles/states on spin button, reset button, and toggle switch.
   - Automated: `nix build .#checks.x86_64-linux.a11y-test`
   - Interactive REPL: `nix run .#checks.x86_64-linux.a11y-test.driverInteractive`
2. **Linux Agent Sandboxing VM Tests** (`nix/tests/sandboxing/{README.md,default.nix,helper.nix}`):
   - Builds the re-executable `sandbox_linux` harness with `craneLib.overrideToolchain rustToolchain` (`helper.nix`).
   - Runs `pkgs.testers.nixosTest` KVM checks (`sandbox-*`) parameterized by `userNamespaces` (enabled vs disabled) to verify Bubblewrap/AppArmor isolation without needing the Zed UI.

## Local Upstream Mirror (`../../docs/zed/nix/`)

- `root-flake.nix`, `build.nix`, `toolchain.nix`
- `dev/flake.nix`
- `modules/{partitions,devshells,overlays,packages,treefmt}.nix`
- `tests/{a11y.nix,a11y_atspi_test.py}`
- `tests/sandboxing/{README.md,default.nix,helper.nix}`
