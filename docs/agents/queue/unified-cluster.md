# Lot: one unified cluster, no distinction between VPS

Order of the owner, 2026-11-21: prd is used like fr1. Load is spread fairly, scaled automatically, and does not distinguish between VPS. Aliases, hostnames, memory and rules are renamed everywhere, as for a real unified cluster.

## Facts read live today

| Machine | Today's alias | CPU | RAM | Disk | OS and ISA |
|---|---|---|---|---|---|
| VPS 1 | `fr1` | 12 cores | 45 GiB | `/srv/xbuild` 805G XFS | Ubuntu, Haswell x86-64-v3 |
| VPS 2 | `prd` | 8 cores, EPYC | about 32 GiB | 300G root | Arch/CachyOS, Zen 4 x86-64-v4 |

- **Load:** at 06:50 fr1 sat at load 30 while prd sat at 3.8. The capacity is there and unused.
- **CI runners already in one pool:** `fr1-org-1`, `fr1-org-2` and `prd-org-1` all carry the label `cluster`, are isolated under `runners/<name>/home`, and fw CI targets `runs-on: cluster`.

## Target

1. **Names.** Nodes get role-free names derived from one declaration in `cluster/build.toml` (cluster name plus index, for example `n1`, `n2`), never per-host roles. This covers:
   - SSH aliases in `~/.ssh/config` on the PC;
   - system hostnames;
   - mesh DNS names;
   - runner names;
   - Cloudflare tunnel names.

   Old aliases stay as secondary aliases until every reference is migrated, then they are removed.
2. **References everywhere.** Every mention of `fr1`, `prd`, `fr2` or the role split is rewritten to the cluster names. That covers:
   - the user's `~/.claude/CLAUDE.md` "Fleet roles" line, as the owner ordered;
   - memory files: fleet-roles, inventory, cluster;
   - skills: `fleet`, `disks`, `containers`;
   - scripts: `fleet-inventory.ts`, `build-cluster.ts`;
   - `cluster/*.toml`, workflows and docs.

   A check fails on any old name outside the history notes.
3. **Equal roles.** Every node runs:
   - builds: sccache-dist servers on every node, with the scheduler pinned by the declaration, not by a role;
   - CI runners, sized by core count;
   - agents;
   - services.

   Binaries target x86-64-v3, so they run on both ISAs. Production services run on more than one node behind Cloudflare load balancing or a tunnel with failover; see `cloudflare-dual-mode-parity.md` for the dual mode.
4. **Fair and automatic placement.** One scheduler places long jobs by live load and free memory and disk, with no fixed host. The choice of tool (fw's own, Nomad, k3s, systemd plus a fw placement daemon) is measured, not assumed. Bursts go to Cloudflare Containers or gcloud when every node is saturated.
5. **Proof.** Run the same job set before and after, and report per-node load and memory with wall time. A failover test stops one node and shows the services still answer, checked by content.
