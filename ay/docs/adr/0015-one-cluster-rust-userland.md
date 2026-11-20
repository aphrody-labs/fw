# ADR-15: One cluster, a Rust userland

Status: proposed 2026-11-20. Extends 0009 and 0014; supersedes nothing. Applies the owner's directive of 2026-11-20 (a Rust userland, one cluster,
merge the VPS; Docker, nginx and systemd are to be replaced, not defended).

## Context
Read live on 2026-11-20 on the three hosts (roles: build, test, prod):
- The userland is already Rust where it counts: uutils coreutils and sudo-rs are installed on all three.
- The hosts are one logical server by convention only: a flat registry (`/etc/fleet/cluster.toml`, rendered by the fleet scripts) over a WireGuard
  mesh, three distros/kernels' worth of drift (the prod host is a different distribution and kernel than the other two), and per-host timers and units.
- Docker and nginx were removed from the build and test hosts on the owner's order; the prod host still runs its whole production on them.
- Disk saturation came from policy gaps, not from the tools (see `tools/host-hygiene.ts`).

## Decision
1. **One cluster, hosts become labels.** The three machines join one cluster description (`cluster.toml` becomes the only source, generated into
   `ay.local.toml` and the hosts' registry). Roles are node labels (`build`, `edge`, `prod`) used by the scheduler; no per-host scripts.
2. **One base.** Every node runs the same distribution and kernel line. The prod host converges last, one node at a time, with its data volumes
   (database, object store) detached from the OS so a node is replaceable.
3. **Runtime: Kiri.** The containers move from Docker to Kiri (Rust, Docker Engine API, so Compose files and CLIs stay). Adoption is gated by the
   proof below, first on the build host (no production), then the test host, then prod.
4. **Edge: one rendered proxy config.** The proxy (Caddy, or a Pingora/Sozu service if Rust-only is required) is rendered from the service registry,
   never edited by hand; it replaces nginx everywhere.
5. **Supervision behind one manifest.** Services and timers are declared once (`ay.toml` / registry) and rendered to the supervisor of the node. systemd
   stays PID 1 until a Rust or small-C supervisor (dinit, s6) boots a node and passes the same proof; the manifest makes the swap a render change.
6. **Userland ladder.** Done: coreutils, sudo. Next: shell (`fw sh`/brush), time sync (ntpd-rs), then the rest as it earns its place by the proof.

## Proof (a unit is done only with its proof)
- Kiri: `docker --context kiri compose up` of the real stack on the build host passes its health checks; `ay hygiene` dry run stays clean.
- Edge: the public health endpoints answer 200 through the rendered proxy; the config is regenerated from the registry on a fresh node.
- One base: `ay disks` and `uname -r` identical on every node; a node rebuilt from the manifest rejoins and takes its labels.
- Supervision: a node boots under the new supervisor and `ay hygiene` timer fires on it.

## Consequences
The prod host is the last to move and never receives a change that has not run on the others. Until step 3 passes, prod keeps Docker and its proxy.
