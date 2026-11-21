#!/usr/bin/env bun
// Off-site backup of production data to R2 through restic, configured by cluster/backup.toml.
//   bun offsite-backup.ts install    (PC) bucket, bucket-scoped token, restic password and escrow, files, units, timer
//   bun offsite-backup.ts run [cfg]  (host) dump, upload, read back, compare, prune; exit 1 on any mismatch
//   bun offsite-backup.ts status     (PC) last snapshots and timer state
import { createHash } from "node:crypto";
import { createReadStream, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { join, posix, resolve } from "node:path";

export type Source =
  | { name: string; kind: "snapshot-dir"; path: string }
  | { name: string; kind: "postgres-docker"; container: string; user: string; database: string }
  | { name: string; kind: "postgres-host"; unix_user: string; port: number; database: string };
export type Config = {
  offsite: {
    host: string; bucket: string; repository_path: string; staging: string; config_dir: string; schedule: string;
    verify_data_subset: string; max_snapshot_age_hours: number; escrow_host: string; escrow_path: string; retired_units: string[];
    keep: { daily: number; weekly: number; monthly: number };
  };
  sources: Source[];
};

const root = resolve(import.meta.dir, "..", "..", "..");
export const HOST_CONFIG = "/etc/offsite-backup/backup.toml";
export const HOST_SCRIPT = "/usr/local/libexec/offsite-backup.ts";

export const parseConfig = (text: string): Config => Bun.TOML.parse(text) as unknown as Config;

export function renderUnits(cfg: Config): { service: string; timer: string } {
  const o = cfg.offsite;
  const service = [
    "[Unit]",
    "Description=Encrypted off-site backup of production databases to R2 (cluster/backup.toml)",
    "Wants=network-online.target",
    "After=network-online.target shenron-container-backup.service",
    "",
    "[Service]",
    "Type=oneshot",
    "UMask=0077",
    "Nice=10",
    "IOSchedulingClass=idle",
    "TimeoutStartSec=3h",
    `ExecStart=/usr/bin/flock -w 7200 /var/lock/aphrody-heavy.lock /usr/local/bin/bun ${HOST_SCRIPT} run ${HOST_CONFIG}`,
    "",
  ].join("\n");
  const timer = [
    "[Unit]",
    "Description=Daily encrypted off-site backup to R2",
    "",
    "[Timer]",
    `OnCalendar=${o.schedule}`,
    "Persistent=true",
    "RandomizedDelaySec=300",
    "",
    "[Install]",
    "WantedBy=timers.target",
    "",
  ].join("\n");
  return { service, timer };
}

export const repositoryUrl = (cfg: Config, accountId: string): string =>
  `s3:https://${accountId}.r2.cloudflarestorage.com/${cfg.offsite.bucket}/${cfg.offsite.repository_path}`;

export function newestSnapshot(names: string[]): string | undefined {
  return names.filter((n) => /^\d{8}T\d{12}Z$/.test(n)).sort().at(-1);
}

export const sha256File = (path: string): Promise<string> =>
  new Promise((ok, fail) => {
    const hash = createHash("sha256");
    createReadStream(path).on("data", (c) => hash.update(c)).on("error", fail).on("end", () => ok(hash.digest("hex")));
  });

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
}

// ---------- host side ----------

function sh(args: string[], opts: { env?: Record<string, string>; stdin?: ReturnType<typeof Bun.file>; out?: string } = {}): string {
  const proc = Bun.spawnSync(args, {
    env: { ...process.env, ...opts.env },
    stdin: opts.stdin,
    stdout: opts.out ? Bun.file(opts.out) : "pipe",
    stderr: "pipe",
  });
  if (proc.exitCode !== 0) throw new Error(`${args.slice(0, 3).join(" ")} exit ${proc.exitCode}: ${proc.stderr.toString().trim().slice(0, 400)}`);
  return opts.out ? "" : proc.stdout.toString();
}

function dumpPostgres(source: Exclude<Source, { kind: "snapshot-dir" }>, dir: string): string[] {
  const custom = join(dir, `${source.name}.dump`);
  const plain = join(dir, `${source.name}.sql.zst`);
  const dumpCmd = (format: "c" | "p"): string[] =>
    source.kind === "postgres-docker"
      ? ["docker", "exec", source.container, "pg_dump", "-U", source.user, `-F${format}`, source.database]
      : ["runuser", "-u", source.unix_user, "--", "pg_dump", "-p", String(source.port), `-F${format}`, source.database];
  sh(dumpCmd("c"), { out: custom });
  const listCmd =
    source.kind === "postgres-docker"
      ? ["docker", "exec", "-i", source.container, "pg_restore", "--list"]
      : ["runuser", "-u", source.unix_user, "--", "pg_restore", "--list"];
  const listing = sh(listCmd, { stdin: Bun.file(custom) });
  if (!listing.includes("TABLE")) throw new Error(`${source.name}: pg_restore --list shows no table`);
  sh(["bash", "-c", `set -o pipefail; ${dumpCmd("p").map((a) => `'${a}'`).join(" ")} | zstd -T2 -q -o '${plain}'`]);
  sh(["zstd", "-t", "-q", plain]);
  return [custom, plain];
}

async function gateSnapshot(source: Extract<Source, { kind: "snapshot-dir" }>, maxAgeHours: number): Promise<string> {
  const name = newestSnapshot(readdirSync(source.path));
  if (!name) throw new Error(`${source.name}: no timestamped snapshot in ${source.path}`);
  const dir = join(source.path, name);
  const manifestPath = join(dir, "VERIFIED.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.sqliteIntegrity !== "ok" || manifest.postgresArchiveList !== "ok" || manifest.redisIntegrity !== "ok") {
    throw new Error(`${source.name}: verification gates are not green`);
  }
  if (Date.now() - statSync(manifestPath).mtimeMs > maxAgeHours * 3600_000) throw new Error(`${source.name}: snapshot older than ${maxAgeHours} h`);
  for (const [file, expected] of Object.entries<string>(manifest.files)) {
    if ((await sha256File(join(dir, file))) !== expected) throw new Error(`${source.name}: local hash mismatch ${file}`);
  }
  return dir;
}

export async function run(cfg: Config): Promise<void> {
  const o = cfg.offsite;
  const env: Record<string, string> = {};
  for (const line of readFileSync(join(o.config_dir, "r2.env"), "utf8").split("\n")) {
    const eq = line.indexOf("=");
    if (eq > 0) env[line.slice(0, eq)] = line.slice(eq + 1);
  }
  env.RESTIC_PASSWORD_FILE = join(o.config_dir, "restic.password");
  env.AWS_DEFAULT_REGION = "auto";
  env.RESTIC_CACHE_DIR = join(o.staging, "restic-cache");
  const restic = (args: string[]): string => sh(["restic", ...args], { env });
  try { restic(["cat", "config"]); } catch { restic(["init"]); }

  const stamp = new Date().toISOString().replace(/[-:.]/g, "").slice(0, 15);
  const stage = join(o.staging, stamp);
  mkdirSync(stage, { recursive: true, mode: 0o700 });
  const roots: string[] = [stage];
  const checksums: Record<string, string> = {};
  try {
    for (const source of cfg.sources) {
      if (source.kind === "snapshot-dir") {
        const dir = await gateSnapshot(source, o.max_snapshot_age_hours);
        roots.push(dir);
        for (const f of walk(dir)) checksums[f] = await sha256File(f);
      } else {
        for (const f of dumpPostgres(source, stage)) checksums[f] = await sha256File(f);
      }
      console.log(`staged ${source.name}`);
    }
    const summary = JSON.parse(
      restic(["backup", "--json", "--tag", "offsite", "--exclude", ".lock", ...roots]).trim().split("\n").filter(Boolean).at(-1)!,
    );
    const id: string = summary.snapshot_id;
    console.log(`uploaded snapshot ${id}: ${summary.total_files_processed} files, ${summary.data_added} bytes added`);

    restic(["check", `--read-data-subset=${o.verify_data_subset}`]);
    const target = join(o.staging, `readback-${stamp}`);
    mkdirSync(target, { recursive: true, mode: 0o700 });
    try {
      restic(["restore", id, "--target", target]);
      let compared = 0;
      for (const [file, expected] of Object.entries(checksums)) {
        const restored = join(target, file);
        if (!existsSync(restored)) throw new Error(`read-back missing ${file}`);
        if ((await sha256File(restored)) !== expected) throw new Error(`read-back checksum mismatch ${file}`);
        compared++;
      }
      console.log(`read-back verified from R2: ${compared} files, sha256 equal to the sources, restic check ${o.verify_data_subset} ok`);
    } finally {
      rmSync(target, { recursive: true, force: true });
    }
    restic(["forget", "--tag", "offsite", "--keep-daily", String(o.keep.daily), "--keep-weekly", String(o.keep.weekly), "--keep-monthly", String(o.keep.monthly), "--prune"]);
    console.log(restic(["snapshots", "--compact", "--tag", "offsite"]));
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}

// ---------- PC side ----------

async function ssh(host: string, script: string, stdin?: string): Promise<string> {
  const proc = Bun.spawn(["ssh", "-o", "BatchMode=yes", host, stdin === undefined ? "bash -s" : script], {
    stdin: new Response(stdin ?? `set -o pipefail\n${script}\n`),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  if (code !== 0) throw new Error(`ssh ${host} exit ${code}: ${err.trim().slice(0, 300)}`);
  return out.trim();
}

const put = (host: string, path: string, mode: string, content: string) =>
  ssh(host, `sudo mkdir -p "$(dirname '${path}')"; sudo tee '${path}' >/dev/null; sudo chmod ${mode} '${path}'`, content);

async function cloudflare(path: string, init: RequestInit = {}): Promise<any> {
  const account = process.env.CLOUDFLARE_ACCOUNT_ID!;
  const res = await fetch(`https://api.cloudflare.com/client/v4/accounts/${account}${path}`, {
    ...init,
    headers: { Authorization: `Bearer ${process.env.CLOUDFLARE_API_TOKEN}`, "Content-Type": "application/json" },
  });
  return res.json();
}

export async function install(cfg: Config): Promise<void> {
  const o = cfg.offsite;
  const account = process.env.CLOUDFLARE_ACCOUNT_ID!;
  const exists = await cloudflare(`/r2/buckets/${o.bucket}`);
  if (!exists.success) {
    const made = await cloudflare("/r2/buckets", { method: "POST", body: JSON.stringify({ name: o.bucket, locationHint: "weur" }) });
    if (!made.success) throw new Error(`bucket: ${JSON.stringify(made.errors)}`);
  }
  await ssh(o.host, "command -v restic >/dev/null || sudo pacman -S --noconfirm --needed restic >/dev/null");
  const haveEnv = (await ssh(o.host, `sudo test -s ${posix.join(o.config_dir, "r2.env")} && echo yes || echo no`)) === "yes";
  if (!haveEnv) {
    const name = `offsite-backup-${o.bucket}`;
    for (const t of (await cloudflare("/tokens?per_page=50")).result ?? []) if (t.name === name) await cloudflare(`/tokens/${t.id}`, { method: "DELETE" });
    const groups = (await cloudflare("/tokens/permission_groups")).result as { id: string; name: string }[];
    const ids = ["Workers R2 Storage Bucket Item Read", "Workers R2 Storage Bucket Item Write"].map((n) => groups.find((g) => g.name === n)!.id);
    const token = await cloudflare("/tokens", {
      method: "POST",
      body: JSON.stringify({
        name,
        policies: [{ effect: "allow", resources: { [`com.cloudflare.edge.r2.bucket.${account}_default_${o.bucket}`]: "*" }, permission_groups: ids.map((id) => ({ id })) }],
      }),
    });
    if (!token.success) throw new Error(`token: ${JSON.stringify(token.errors)}`);
    const secret = new Bun.CryptoHasher("sha256").update(token.result.value).digest("hex");
    await put(o.host, posix.join(o.config_dir, "r2.env"), "600", `RESTIC_REPOSITORY=${repositoryUrl(cfg, account)}\nAWS_ACCESS_KEY_ID=${token.result.id}\nAWS_SECRET_ACCESS_KEY=${secret}\n`);
  }
  const passwordPath = posix.join(o.config_dir, "restic.password");
  await ssh(o.host, `sudo test -s ${passwordPath} || (head -c 32 /dev/urandom | base64 | sudo tee ${passwordPath} >/dev/null; sudo chmod 600 ${passwordPath})`);
  // Escrow: the repository is unreadable without this key, so a second copy lives on another host.
  const password = await ssh(o.host, `sudo cat ${passwordPath}`);
  await ssh(o.escrow_host, `mkdir -p "$(dirname '${o.escrow_path}')"; umask 077; cat > '${o.escrow_path}'`, password);
  await put(o.host, HOST_CONFIG, "600", await Bun.file(join(root, "cluster", "backup.toml")).text());
  await put(o.host, HOST_SCRIPT, "755", await Bun.file(import.meta.path).text());
  const { service, timer } = renderUnits(cfg);
  await put(o.host, "/etc/systemd/system/offsite-backup.service", "644", service);
  await put(o.host, "/etc/systemd/system/offsite-backup.timer", "644", timer);
  const retired = o.retired_units.map((u) => `sudo systemctl disable --now ${u} 2>/dev/null || true`).join("; ");
  await ssh(o.host, `sudo systemctl daemon-reload; ${retired}; sudo systemctl enable --now offsite-backup.timer; systemctl list-timers offsite-backup.timer --no-pager | head -3`);
}

if (import.meta.main) {
  const [command = "status", path] = Bun.argv.slice(2);
  if (command === "run") await run(parseConfig(readFileSync(path ?? HOST_CONFIG, "utf8")));
  else {
    const cfg = parseConfig(await Bun.file(join(root, "cluster", "backup.toml")).text());
    if (command === "install") await install(cfg);
    else console.log(await ssh(cfg.offsite.host, `systemctl list-timers offsite-backup.timer --no-pager | head -3; journalctl -u offsite-backup -n 15 --no-pager | cut -c1-200`));
  }
}
