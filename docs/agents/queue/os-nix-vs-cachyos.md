# os-nix-vs-cachyos: settle the host and image base, then finish the WSL kernel item (queued)

Phase 3 (reproducible OS). Owner: unassigned until the Phase 1 merge lands; the fleet worker records the host facts. Release v2026.11.0: not blocking.

## Goal
Two decisions pull apart. The roadmap and `docs/research/cluster-stack-2026-11-21.md` choose CachyOS repos on both VPS and a CachyOS-based image built from `os/`, and reject Talos. agy conversation 72d50cfe (2026-11-19) built the other direction: a Nix flake (`flake.nix`, `nix/modules/vps-common.nix` running `/bin/fw serve`) and a plan for a custom WSL kernel and WSLg. All three exist on fw `origin/main`.

## Decision to record (recommended)
CachyOS image per the roadmap is the OS artifact. Keep the Nix flake only as a build and dev environment for the `fw` package and the Rust userland if a job actually uses it; delete `nix/modules` host configuration that the CachyOS image replaces. Record this in an ADR under `docs/` and in `os/`.

## Do
1. Check what consumes `flake.nix` today (CI, scripts, docs); delete the host modules nothing reads.
2. Write the ADR with the numbers: image build time, closure or package size, reproducibility result on two builds.
3. WSL item from 72d50cfe (custom kernel and WSLg without legacy dependencies): re-scope as "boot the Phase 3 image under WSL2 and QEMU"; no separate kernel fork until the image gate exists.

## Acceptance
ADR merged; two independent builds of the chosen image are byte-identical (Phase 3 gate); no host config in `nix/` without a consumer.
