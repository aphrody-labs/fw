#!/usr/bin/env bun
// Full inventory of fleet hosts over ssh: disks, LVM, mounts, services, containers, listeners, timers, tunnels, data sizes.
//   bun fleet-inventory.ts [host ...] [--json]      default hosts: fr1 prd
// Every section is read live from the host; nothing is cached or assumed.

export const SECTIONS = [
  "host",
  "block",
  "mounts",
  "lvm",
  "docker",
  "listeners",
  "services",
  "timers",
  "user-units",
  "cron",
  "users",
  "sizes",
  "mesh",
  "tunnels",
  "firewall",
] as const;
export type Section = (typeof SECTIONS)[number];
export type Inventory = { host: string; sections: Record<Section, string[]> };

export const REMOTE_SCRIPT = String.raw`
S() { echo "##$1"; }
S host; hostname; . /etc/os-release; echo "$PRETTY_NAME"; uname -r; echo "cpus=$(nproc) $(lscpu | sed -n 's/^Model name: *//p')"; free -h | sed -n 2p
S block; lsblk -e7 -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS
S mounts; df -hT -x tmpfs -x devtmpfs -x squashfs -x overlay
S lvm; sudo -n vgs 2>&1; sudo -n lvs 2>&1; sudo -n pvs 2>&1
S docker; sudo -n docker ps -a --format '{{.Names}}|{{.Image}}|{{.Status}}|{{.Ports}}' 2>&1; sudo -n docker system df 2>&1; sudo -n docker volume ls -q 2>&1; sudo -n podman ps -a 2>&1 | head -20
S listeners; sudo -n ss -tlnpH 2>&1 | awk '{print $4, $6}' | sed 's/users:((//; s/))//' | cut -c1-120
S services; systemctl list-units --type=service --state=running --no-legend | awk '{print $1}'
S timers; systemctl list-timers --all --no-legend | awk '{print $(NF-1), "->", $NF}'
S user-units; for u in $(ls /home); do for f in /home/$u/.config/systemd/user/*.service /home/$u/.config/systemd/user/*.timer; do [ -e "$f" ] && echo "$u $(basename $f)"; done; done 2>/dev/null
S cron; sudo -n ls /etc/cron.d 2>/dev/null; sudo -n crontab -l 2>/dev/null | grep -vE '^\s*(#|$)'
S users; getent passwd | awk -F: '$3>=1000 && $3<65000 && $7 !~ /nologin|false/ {print $1, $6}'
S sizes; sudo -n timeout 240 du -xsh /home/* /root /opt /srv/* /var/lib/* /var/cache /var/log /tmp /usr 2>/dev/null | sort -rh | head -30
S mesh; sudo -n wg show 2>&1 | grep -E 'interface|peer|allowed|handshake|endpoint'; ip -br a
S tunnels; ps -eo args | grep -E '[c]loudflared'
S firewall; sudo -n nft list ruleset 2>/dev/null | head -20
`;

export function parseSections(output: string): Record<Section, string[]> {
  const sections = Object.fromEntries(SECTIONS.map((s) => [s, [] as string[]])) as Record<Section, string[]>;
  let current: Section | undefined;
  for (const line of output.replace(/\r/g, "").split("\n")) {
    if (line.startsWith("##")) {
      const name = line.slice(2) as Section;
      current = (SECTIONS as readonly string[]).includes(name) ? name : undefined;
      continue;
    }
    if (current && line.trim()) sections[current].push(line);
  }
  return sections;
}

export async function inventory(host: string): Promise<Inventory> {
  const proc = Bun.spawn(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15", host, "bash -s"], {
    stdin: new Response(REMOTE_SCRIPT),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, , code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  if (code !== 0 && !stdout.includes("##host")) throw new Error(`ssh ${host} exited ${code}`);
  return { host, sections: parseSections(stdout) };
}

export function render(inv: Inventory): string {
  const out = [`# ${inv.host}`];
  for (const s of SECTIONS) out.push(`## ${s}`, ...inv.sections[s].map((l) => `  ${l}`));
  return out.join("\n");
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const json = args.includes("--json");
  const hosts = args.filter((a) => !a.startsWith("--"));
  const results = await Promise.all((hosts.length ? hosts : ["fr1", "prd"]).map(inventory));
  console.log(json ? JSON.stringify(results, null, 2) : results.map(render).join("\n\n"));
}
