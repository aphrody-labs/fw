# prd audit, 2026-11-17 (read-only, over ssh; nothing on the host was changed)

Trigger: the owner judged the production host old and badly managed. Method: `pacman`, `checkupdates` (temporary
database), `systemctl`, `journalctl`, `ss`, `docker`, `nft`, `df`, reads of config files. No secret, address or key is
recorded here. Re-run the same reads before acting; this is a snapshot.

## Corrections after the fact-check of the same day (the first version of this page was wrong on two points)
- WRONG, removed: "the distribution is current, 0 updates pending". `checkupdates` had failed (missing fakeroot, exit 1) and I had
  hidden its error output. `pacman -Qu` against the local sync database (synced 2026-11-13 to 2026-11-14) lists 85 pending
  packages, among them `linux` 7.2.14 to 7.3.4, `openssh` 10.6p1-2 to 10.6p2-1, `systemd` 262.1 to 262.2 and `nginx`
  1.30.6-2 to -3; the last full upgrade was 2026-11-07. The host is behind, as the owner said; the real number today is
  higher because the database is older than the repositories.
- WRONG, corrected: "installed 2026-10-15". That is the build date of the Arch cloud image (the first pacman.log line is the
  image builder's `pacman -b`); the machine id and the first login record are from 2026-11-01.
- The running kernel 7.2.14 is also one patch behind the 7.2 stable line (7.2.15 on kernel.org today), as well as one cycle
  behind 7.3.4 and two behind mainline 7.4-rc3.
- Confirmed: backups run (`fleet-backup.timer`, last run 03:43 UTC that day, to R2, age-encrypted), 0 failed units, swap on zram.

## What is badly managed (evidence)
| # | Finding | Evidence |
|---|---|---|
| 1 | Production doubles as a test, build and desktop host, against the fleet rule (`prd` = production only) | containers `rg-e2e-azalee` (rust:latest) and `rg-e2e-pg` running; `rg-nie-xvfb.service`; users `builduser` and `agent` with tmux sessions open for days; AUR helper and `xorg-server-xvfb` installed |
| 2 | A leftover unit restarts every few seconds | `bunos-helium-preview.service` fails at EXEC because `/usr/bin/helium-browser` does not exist; it produced 22 199 of 23 389 error lines in 24 h (95 %); `fleet-notify` and `fleet-health` add about 1 170 more |
| 3 | A third-party desktop repository is enabled on a server | `[omarchy]` in pacman.conf (7 packages installed: editor, shell widgets, themes, keyring), plus `[multilib]` |
| 4 | No host firewall | `ufw`, `firewalld` and `nftables` services inactive; iptables policies ACCEPT; the rules present are Docker's own; listening on all interfaces: 22, 80, 443, 8443, 9443 (nginx) and 8780 (a Bun process). A provider-level firewall was not checked |
| 5 | sshd files contradict each other, the effective result is safe | the main file says `PermitRootLogin yes` and `PasswordAuthentication yes`; `sshd -T` reports `prohibit-password`, `PasswordAuthentication no`, public keys on, because the drop-ins are read first. Only the dead lines in the main file need removing |
| 6 | Floating image tags | `rust:latest` and `cloudflare/cloudflared:latest` run unpinned; `docker system df` reports 24 images (22.4 GB, 18 % reclaimable). Where container data lives (layers or volumes) was not established: `docker system df` shows 47.7 GB of container size while `/var/lib/docker` is 4.4 GB, so the figures disagree and need a per-container check |
| 7 | Host configuration is not under version control | no etckeeper or git in `/etc`; units named `fleet-*` and scripts in `/usr/local/bin` have no source reference on the host |
| 8 | Hosting is by tunnels and one nginx | public hostnames resolve to Cloudflare tunnels (`cloudflared-*` services); the host's own IP exposure is only what finding 4 lists |

## Plan (nothing executed)
0. Update the host: 85 or more pending packages including the kernel, openssh and systemd (needs a reboot window for the kernel; the owner's target kernel is described in the kernel-watch plan).
Because traffic enters through Cloudflare tunnels, a replacement host can take over by running the same connectors; no DNS
record changes, which keeps the frozen zones untouched.
1. Move what does not belong: e2e containers, xvfb, `builduser`, the browser unit and the `agent` sessions go to fr1/fr2
   (the build and test hosts). Production keeps nginx, the site and bot stacks, their databases and the tunnels.
2. Describe the host as code in a repository: packages (explicit list, no desktop repo), units, nginx, nftables
   (default-deny inbound except the ports the tunnels do not cover), sshd (one file, key-only, no root password),
   Docker images pinned by digest, data on named volumes (after the per-container check of finding 6) with backups tested by a restore.
3. Build the new host from that description on fr1 as a VM or a container image, boot-test it, then provision a fresh VPS
   from it. Cut over by starting its connectors and stopping the old ones one service at a time, proving each site with a
   DOM or API check, then keep the old host for a rollback window.
4. Only after step 3 touch the old host (retire it); until then no write on it except what the owner asks for explicitly,
   because a wrong firewall or sshd change on the live host takes both sites down.

Open questions are not asked here: the order follows from the evidence; the owner's decision is the go for step 3 (a new
VPS is a purchase) and for any write on the live host.
