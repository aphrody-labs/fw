# Session review and fact-check, 2026-11-17

Scope: every factual claim made to the owner in this session that was not already proven by a command in the same
reply, re-checked with live reads on 2026-11-17. Status: **proved** (command output read), **corrected** (the claim was
wrong or unsupported and is fixed where it was recorded), **open** (not verified, stated as such).

| # | Claim | Status | Evidence or correction |
|---|---|---|---|
| 1 | prd packages are current, 0 updates pending | **corrected: false** | `checkupdates` failed silently (missing fakeroot, exit 1) and I hid its stderr; `pacman -Qu` lists 85 pending, among them linux 7.2.14 to 7.3.4, openssh, systemd, nginx. Fixed in `prd-audit.md` |
| 2 | prd installed 2026-10-15 | **corrected** | that is the Arch cloud image build date; machine id and first login are 2026-11-01 |
| 3 | prd's crash-looping browser unit | **corrected** | the unit is `bunos-helium-preview.service`, failing at EXEC because the binary is absent (restart loop); journal figures were right |
| 4 | prd sshd contradicts itself, effective state unknown | **corrected** | `sshd -T`: prohibit-password, no password auth, keys on. Safe; only dead lines remain |
| 5 | prd has no firewall | **partly proved** | host services ufw, firewalld, nftables inactive, iptables policies ACCEPT, only Docker rules; provider-level firewall **open** |
| 6 | Kernel 7.2.14 is one cycle behind stable 7.3.4 and two behind mainline | **proved** | kernel.org releases.json: mainline 7.4-rc3, stable 7.3.4 and 7.2.15, longterm 6.18.63 |
| 7 | Linux 7.5 is not released, mainline is 7.4-rc3 | **proved** | tags of torvalds/linux and releases.json |
| 8 | Android 17 exists in AOSP, Android 18 only as a kernel | **proved** | `git ls-remote` of platform/manifest: tag android-17.0.0_r1 and branch android-latest-release; kernel/common branch android18-7.3; no android-18 manifest ref |
| 9 | redroid has no Android 17 image | **proved** | Docker Hub tags filtered on 17 return only an old r220817 tag; newest 16.0.0 |
| 10 | CachyOS rc is at 7.4.rc3 | **proved, with a caveat** | CachyOS/linux-cachyos `.SRCINFO` says 7.4.rc3; the AUR package linux-cachyos-rc says 7.4.rc2: channels differ, the watch uses GitHub |
| 11 | ADR-0010 used the command `bun test --target workerd` | **corrected: invented** | `bun test` has no `--target`; replaced by "a workerd run provided by the plugin, command not designed" |
| 12 | Cloudflare capability statements of the Bun map | **partly proved** | 8 of 54 features now carry a docs source read today; 46 are claims from memory and counted as unverified by `bun tools/cloud-map.ts`. Two statements were searched and not found: `eval`/`new Function` blocked, and no UDP on Workers. The TCP entry was wrong in one respect: the docs describe an inbound `connect()` handler too |
| 13 | Inventory: 1011 repos in aphrody-labs, 937 forks; 365 crates; 618 npm packages; 4 zones, 11 Workers | **proved** | each from a command run in the session (`gh repo list`, crates.io API, npm registry search, `cf`); the npm count comes from a search capped at 1000 results, so it is a lower bound |
| 14 | Mirror crawl: 19 773 pages, 8 gone, tree reproducible | **corrected, then partly proved** | two builds of one store differed because the server's read-through had stored 10 more files in between; fixed by crawling scripts and companion files (tests pass); the completing crawl on fr2 is still running and the post-fix tree comparison is **open** |
| 15 | bun.com answers 404 to the og images | **proved** | live requests return 404 |
| 16 | Fork is 0 behind oven-sh | **corrected** | today: ahead 4486, behind 2 (upstream moved since the merge) |
| 17 | Fork CI green on the merge | **proved** | `native` success on f618a7920 and later, merge run cancelled by newer pushes; the run on 4407879a5 was still in progress when read |
| 18 | rust-mono: 51 forks, CI green, sync failing on the app key | **proved** | `.gitmodules` has 51 entries; `ci` success on 5e789b7; `sync` failure 2026-11-17T04:48Z; the key problem itself is the agent's diagnosis (**open**: I did not read the secret) |
| 19 | kernel-watch workflow green, versions table | **proved for runs, open for figures** | 5 recent runs success; the table was cross-checked on mainline, stable, AUR linux-mainline and CachyOS rc; Alpine, Fedora, NixOS, Ubuntu, Android and ChromiumOS rows were not re-read by me |
| 20 | aphrody CI green on 0845a505cb | **proved** | `gh run list` success; the cause analysis (binaryen v133) is the agent's, not re-run by me |
| 21 | Gemini's Zed-based desktop exists | **proved** | aphrody crates/os_desktop (3 632 lines, binary aphrody-os), gpui_linux, gpui_app_shell, commits 64117c9b67 and db669c43f9; whether it runs as a Wayland compositor or as a client shell on one was **not** established (it describes itself as shell plus compositor canvas) |
| 22 | WSL kernel 6.18.58.1, WSLg 1.0.79, no distro installed | **proved** | `wsl --version` on the PC; Microsoft's newest release tag 6.18.58.1 |
| 23 | Hyperfine import blocked by the main-integrity ruleset | **relayed, not checked** | message from the browser session only |

## Still open (not verified by me)
- Whether prd has a provider-level firewall; where container data lives (layers or volumes).
- The first and later results of the three running agents: arch-rc-wsl, bun-native-audit, unified-style.
- Real R2/S3 test of the mirror, and hosting it under bun.aphrody (DNS frozen).
- The 46 unsourced entries of the Bun-to-Cloudflare map, and every `parity` value (proof comes from the conformance suite).
- Android 17 or later booting in WSL, QEMU or Docker: not attempted yet.
