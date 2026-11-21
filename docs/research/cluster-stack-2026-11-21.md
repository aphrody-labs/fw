# Cluster stack (2026-11-21)

Scope: fr1 (OVH, Haswell x86-64-v3, 12 vCPU, 45 GB, root 300 GB plus `$WORK=/srv/xbuild` 805 GB XFS on LVM) and prd (Hostinger, EPYC 9354P Zen 4, x86-64-v4, 8 vCPU, 31 GB, 400 GB). The two hosts are linked by a WireGuard mesh `10.200.77.0/24`. Around them sit Cloudflare (Workers, R2, containers), gcloud (Cloud Build, Batch, spot GPU) and the owner's RTX 4070 PC. Production is moving from prd to Cloudflare, so both VPS become build and agent hosts.

Sources read today:
- the `gh api repos/<r>/releases/latest` results for each component (versions in the table);
- Context7 `/k3s-io/docs` (distributed-multicloud.md, advanced.md);
- the `fleet`, `disks`, `ai-os-orchestrator` and `work` skills;
- the fw crate `fw-cluster`: `fw-k8s`, kube 5.1.0.

## Decision

| Need | Choice | Version (released) | Why |
|---|---|---|---|
| Distributed Rust and C/C++ compile | `sccache-dist` scheduler on fr1, servers on fr1 and on prd inside a systemd slice (`Nice=19`, `CPUWeight=15`, `MemoryHigh=12G`) | sccache from the pinned toolchain | Cargo has no RE-API; this is the only way to use both VPS for one cargo build. It is already named in the fleet skill. |
| Remote execution and cache (Bazel, siso/Chromium, Buck2, reclient) | NativeLink: server and worker already running on fr1, plus a worker on prd | v1.8.7 (2026-11-20) | Already installed. Gives Chromium/siso builds cache hits across hosts. |
| Agent and job queue | existing `apps/agents/runner.ts` (detached `tmux` sessions on the hosts), fan-out ≤ 3 | in-repo | Already the documented path; no new scheduler. |
| CI runners | self-hosted GitHub Actions runners as systemd services on fr1 (one already serves shenron) and in a prd slice | n/a | ARC (gha-runner-scale-set 0.15.1) needs Kubernetes, which is not worth it for 2 nodes. |
| Mesh | keep the WireGuard `10.200.77.0/24` | kernel | Works today; NetBird 0.83.2 / Headscale 0.29.7 only if more nodes join. |
| Shared trees (zero copy) | one checkout per repo at `$WORK/src/<repo>` on fr1, NFSv4.2 export over the mesh to prd, `cp --reflink=auto` on XFS | kernel | One checkout per repo, zero duplication. |
| Cloud and DNS IaC | OpenTofu with Cloudflare, Google and OVH providers | v1.13.9 (2026-11-05) | Declarative, open licence (Terraform is BSL). |
| Secrets | SOPS + age in the repos; Cloudflare and GCP secret stores for runtime | sops v3.13.3 | No server to run. OpenBao 2.7.3 only if dynamic secrets are needed. |
| Metrics and logs | VictoriaMetrics + VictoriaLogs (single binaries) + Grafana; OTel collector on each host | VM v1.156.0, VL v1.54.0, Grafana 13.3.1, otelcol 0.165.0 | Smallest footprint for 2 hosts. |
| Backups | restic to R2 | v0.20.0 (2026-11-19) | Incremental and encrypted; R2 has no egress fees. |
| Burst compute | gcloud Cloud Build and Batch, spot L4 VMs, behind a budget cap | n/a | Uses the credits without competing with fr1. |
| Host OS | CachyOS repos (x86-64-v3 on fr1, v4/znver4 on prd), BORE/scx, tuned through `fw tune cachy` | n/a | User request; prd already runs the CachyOS kernel 7.4.0-rc3. |
| Control CLI | `fw` (fw-cluster/fw-k8s for any k8s, fleet scripts for hosts) | fw 4.2.0 | fw is the core of the stack. |

## Rejected for now (re-evaluate if services return to the VPS)

- **k3s** v1.37.2+k3s1 (2026-11-03). Multicloud works through `--flannel-backend=wireguard-native --flannel-external-ip` plus `--node-external-ip` (k3s docs, distributed-multicloud.md), and GPU nodes need the NVIDIA container runtime (k3s docs, advanced.md). With production on Cloudflare, two build nodes gain little from a control plane that costs memory and operations work.
- **Talos** v1.14.6: an immutable OS that would replace CachyOS.
- **Nomad**: BSL licence.
- **Argo CD** 3.6.2 / **Flux** 2.10.1: they only make sense with k3s.
- **Cilium** 1.20.5: overkill for 2 nodes.
- **The PC as a cluster GPU node:** it runs Windows. CUDA in WSL2 works for jobs, but k3s Windows or WSL GPU scheduling is fragile, so GPU jobs go to the PC through `fw` directly or to gcloud spot L4.

## Measures to collect when deployed

- fw release build time: fr1 alone against fr1 + prd with sccache-dist (target: about 1.5x faster or better).
- NativeLink cache hit rate on a Chromium siso build.
- prd production p95 latency during a build burst. It must not move while prod is still there.
- `$WORK` usage kept at or below 80% (the XFS allocator stalls above 90%).
