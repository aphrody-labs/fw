#!/usr/bin/env bun
// Disk inventory of the fleet: every block device, partition, LVM volume, network mount and filesystem, with free
// space, and where to put large work (builds, caches, mirrors). Reads hosts from ay.local.toml [hosts].
//   bun tools/disks.ts                 table per host + the best place for large work
//   bun tools/disks.ts --json          machine-readable result
//   bun tools/disks.ts <build-host> <prod-host>         only these hosts
//   bun tools/disks.ts --usage <path>  biggest directories under <path> on each host (dust, else du)
import { readAy } from "./local.ts";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

export type Mount = { source: string; fstype: string; size: number; used: number; avail: number; target: string };
export type Device = { name: string; size: string; type: string; fstype: string | null; mountpoints: string[]; children?: Device[] };
export type HostDisks = {
  host: string;
  devices: Device[];
  mounts: Mount[];
  lvm: { vg: string; size: string; free: string }[];
  tools: string[];
  error?: string;
};

const root = resolve(import.meta.dir, "..");
const SKIP_FS = new Set(["tmpfs", "devtmpfs", "overlay", "squashfs", "efivarfs", "proc", "sysfs", "cgroup2"]);

/** Parses `df -B1 --output=source,fstype,size,used,avail,target`. */
export function parseDf(text: string): Mount[] {
  return text
    .trim()
    .split("\n")
    .slice(1)
    .map(l => l.trim().split(/\s+/))
    .filter(f => f.length >= 6 && !SKIP_FS.has(f[1]!))
    .map(f => ({
      source: f[0]!,
      fstype: f[1]!,
      size: Number(f[2]),
      used: Number(f[3]),
      avail: Number(f[4]),
      target: f.slice(5).join(" "),
    }));
}

/** The mount with the most free space that isn't boot or EFI: where large work goes. */
export function bestForWork(mounts: Mount[]): Mount | undefined {
  return mounts
    .filter(m => !/^\/(boot|efi)(\/|$)/.test(m.target))
    .sort((a, b) => b.avail - a.avail)[0];
}

const gb = (n: number) => `${(n / 1e9).toFixed(0)} GB`;

async function sh(host: string, script: string): Promise<string> {
  const p = Bun.spawn(["timeout", "90", "ssh", host, script], { stdin: "ignore", stdout: "pipe", stderr: "pipe" });
  const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
  if (code !== 0) throw new Error(err.trim() || `ssh ${host} exited ${code}`);
  return out;
}

const SEP = "\n@@@\n";
async function scan(host: string): Promise<HostDisks> {
  const script = [
    "lsblk -J -e7 -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS",
    "df -B1 --output=source,fstype,size,used,avail,target",
    "sudo -n vgs --noheadings --units g -o vg_name,vg_size,vg_free 2>/dev/null || true",
    "for t in lsblk parted sgdisk growpart resize2fs xfs_growfs btrfs lvm pvs mkfs.xfs mkfs.ext4 dust ncdu duf; do command -v $t >/dev/null && echo $t; done; true",
  ].join(`; printf '${SEP}'; `);
  try {
    const [lsblk, df, vgs, tools] = (await sh(host, script)).split(SEP);
    return {
      host,
      devices: (JSON.parse(lsblk!).blockdevices ?? []) as Device[],
      mounts: parseDf(df!),
      lvm: vgs!.trim().split("\n").filter(Boolean).map(l => {
        const [vg, size, free] = l.trim().split(/\s+/);
        return { vg: vg!, size: size!, free: free! };
      }),
      tools: tools!.trim().split("\n").filter(Boolean),
    };
  } catch (e) {
    return { host, devices: [], mounts: [], lvm: [], tools: [], error: (e as Error).message };
  }
}

function hosts(argv: string[]): string[] {
  const named = argv.filter(a => !a.startsWith("--") && !a.startsWith("/"));
  if (named.length) return named;
  const ay = readAy(root) as unknown as { hosts?: Record<string, string> };
  return [...new Set(Object.values(ay.hosts ?? {}))];
}

function devLines(d: Device, depth = 0): string[] {
  const mp = d.mountpoints?.filter(Boolean).join(",") ?? "";
  const self = `${"  ".repeat(depth)}${d.name} ${d.size} ${d.type}${d.fstype ? ` ${d.fstype}` : ""}${mp ? ` → ${mp}` : ""}`;
  return [self, ...(d.children ?? []).flatMap(c => devLines(c, depth + 1))];
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  const usageAt = argv.indexOf("--usage");
  const list = hosts(argv);
  if (usageAt >= 0) {
    const path = argv[usageAt + 1] ?? "/";
    const out = await Promise.all(
      list.map(async h => `== ${h} ${path}\n${await sh(h, `(command -v dust >/dev/null && dust -d 1 -n 15 ${path} 2>/dev/null) || sudo -n du -xh -d1 ${path} 2>/dev/null | sort -h | tail -15`).catch(e => String(e))}`),
    );
    console.log(out.join("\n"));
    process.exit(0);
  }
  const all = await Promise.all(list.map(scan));
  if (argv.includes("--json")) {
    console.log(JSON.stringify(all, null, 2));
    process.exit(0);
  }
  for (const h of all) {
    console.log(`\n== ${h.host}${h.error ? `  (error: ${h.error})` : ""}`);
    for (const d of h.devices) for (const l of devLines(d)) console.log(`  ${l}`);
    for (const m of h.mounts) console.log(`  ${m.target.padEnd(28)} ${m.fstype.padEnd(6)} ${gb(m.avail).padStart(7)} free of ${gb(m.size)}  (${m.source})`);
    for (const v of h.lvm) console.log(`  LVM ${v.vg}: ${v.size} total, ${v.free} free in the volume group`);
    const best = bestForWork(h.mounts);
    if (best) console.log(`  → large work: ${best.target} (${gb(best.avail)} free, ${best.fstype})`);
  }
}
