---
name: fleet
description: "Run a command, schedule builds, deploy or check on the fleet (fr1 heavy builds, prd production), gcloud Cloud Build. Use for any remote ssh/scp/systemctl/build; never a heavy build on the PC CPU."
---
> Cloudflare compute and the local RTX 4070; fr2 is out of service.


1. Always check live capacity and peer locks first: inspect `fleet_status` (or `bun run fleet:lb probe --all`) and `relay` `session_list` / `session_inbox` before writing under the shared build dir (`<work>` of the host) or launching heavy Cargo/C++ builds on `fr1`.
2. Launch every long remote command (builds, installs, syncs, benchmarks, kernel compiles) directly inside a detached `tmux` session via `bun "$HOME/.gemini/scripts/tmux-run.ts" start <host> <job> "<cmd>"` and inspect non-blockingly with `status <host> <job>`. For short one-liners, pipe single-quoted heredocs through `@' ... '@ | ssh <host> "tr -d '\r' | bash"`.
3. Exit 255 / slow host: `@' ... '@ | ssh <fr1> "tr -d '\r' | bash"` checking `uptime` (and clearing stale processes only when owned by your task/session).
4. Deploy a file: `scp -q <src> <host>:<work>/X/` after ensuring `<work>/X` ownership via heredoc (`<work>` from `assets/os/fleet_cluster.toml` or `ay.toml`).
5. Service: `ssh prd "sudo systemctl restart <svc> && systemctl is-active <svc>"`.
6. Prove a deploy with content, not a status: rendered page or real API response payload check; bare HTTP 200 proves nothing.
7. Never break prod: verify on build/test host before `prd`. Detailed roles: memory fleet-roles.md.
8. Compute targets, in order of use: `fr1` (12c/47G primary, `nativelink`/`sccache-dist` scheduler) for heavy builds, CI and agents; `prd` (8c AMD EPYC 9354P/32G) for production only, bursts isolated in a systemd slice (`Nice=19`, `IOSchedulingClass=idle`, `CPUWeight=15`, `MemoryHigh=12G`); gcloud Cloud Build (`gcloud builds submit`) for builds that exceed `fr1`; Cloudflare Workers, R2 and containers for edge compute and artifacts; the local RTX 4070 for GPU work (CUDA, wgpu, model inference). Build with `RUSTC_WRAPPER=sccache`, `mold` (release/LTO) or `wild` (dev/test), `CARGO_INCREMENTAL=0`.
9. `fr2` (6c/11.7G worker, NFSv4.2 `nconnect=8`) is out of service: unreachable, assign nothing to it, skip it in `fleet_status`; tests, bench and research go to `fr1`, gcloud or Cloudflare until it returns. Anything that lived on its NFS `<work>/shared` or its services (SearXNG, credentials) is unavailable meanwhile.
