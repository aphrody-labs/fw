# prd audit, 2026-11-17 (read-only, over ssh; nothing on the host was changed)

Trigger: the owner judged the production host old and badly managed. Method: `pacman`, `checkupdates` (temporary
database), `systemctl`, `journalctl`, `ss`, `docker`, `nft`, `df`, reads of config files. No secret, address or key is
recorded here. Re-run the same reads before acting; this is a snapshot.

## What is not the problem
- The distribution is current: Arch Linux rolling, installed 2026-10-15, last full upgrade 2026-11-07, `checkupdates`
  reports 0 pending, 720 packages, 0 failed units, running kernel equals the installed `linux` package (7.2.14).
- The kernel is one cycle behind kernel.org stable (7.3.4) and two behind mainline (7.4-rc3): that is Arch's own packaging
  delay, not neglect.
- Backups exist: `fleet-backup.timer` runs every few hours (last run 03:43 UTC today) to R2 with age encryption; 3 btrfs
  subvolumes; swap on zram.

## What is badly managed (evidence)
| # | Finding | Evidence |
|---|---|---|
| 1 | Production doubles as a test, build and desktop host, against the fleet rule (`prd` = production only) | containers `rg-e2e-azalee` (rust:latest) and `rg-e2e-pg` running; `rg-nie-xvfb.service`; users `builduser` and `agent` with tmux sessions open for days; AUR helper and `xorg-server-xvfb` installed |
| 2 | A crash-looping browser unit fills the journal | `(helium-browser)` produced 22 199 of 23 389 error lines in 24 h (95 %); `fleet-notify` and `fleet-health` add about 1 170 more |
| 3 | A third-party desktop repository is enabled on a server | `[omarchy]` in pacman.conf (7 packages installed: editor, shell widgets, themes, keyring), plus `[multilib]` |
| 4 | No host firewall | iptables policies INPUT/FORWARD/OUTPUT all ACCEPT; nftables holds only Docker's NAT; listening on all interfaces: 22, 80, 443, 8443, 9443 (nginx) and 8780 (a Bun process) |
| 5 | sshd configuration contradicts itself | main file sets `PermitRootLogin yes` and `PasswordAuthentication yes`; drop-ins set `prohibit-password` and `no`; the Include is on line 2 so the drop-ins should win, but the effective values were not confirmed with `sshd -T` |
| 6 | Floating image tags | `rust:latest` and `cloudflare/cloudflared:latest` run unpinned; `docker system df` reports 24 images (22.4 GB, 18 % reclaimable). Where container data lives (layers or volumes) was not established: `docker system df` shows 47.7 GB of container size while `/var/lib/docker` is 4.4 GB, so the figures disagree and need a per-container check |
| 7 | Host configuration is not under version control | no etckeeper or git in `/etc`; units named `fleet-*` and scripts in `/usr/local/bin` have no source reference on the host |
| 8 | Hosting is by tunnels and one nginx | public hostnames resolve to Cloudflare tunnels (`cloudflared-*` services); the host's own IP exposure is only what finding 4 lists |

## Plan (nothing executed)
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
