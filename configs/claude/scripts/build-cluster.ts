#!/usr/bin/env bun
// Build cluster as code: sccache-dist scheduler and servers across the hosts of cluster/build.toml.
//   bun build-cluster.ts plan              print the rendered configs (secrets redacted)
//   bun build-cluster.ts apply             install binary, secrets, configs and systemd units on every host, start them
//   bun build-cluster.ts status            scheduler status and per-host unit state
// Every name, address, port and path comes from cluster/build.toml. Secrets are generated on first apply, kept in
// /etc/sccache-dist/secrets.json (root, 0600) on the scheduler host and never printed.
import { join, resolve } from "node:path";
import { posix } from "node:path";

export type Host = {
  ssh: string;
  address: string;
  user: string;
  work: string;
  vcpu: number;
  target_cpu: string;
  slice: string;
  roles: string[];
};
export type ClusterConfig = {
  mesh: { cidr: string };
  scheduler: { host: string };
  sccache_dist: { scheduler_port: number; server_port: number; version: string; build_target_cpu: string; tools_dir: string };
  nativelink: { scheduler_port: number; cas_port: number };
  hosts: Record<string, Host>;
};
export type Secrets = { clientToken: string; hs256Key: string; serverTokens: Record<string, string> };

const root = resolve(import.meta.dir, "..", "..", "..");

export function loadConfig(path = join(root, "cluster", "build.toml")): Promise<ClusterConfig> {
  return Bun.file(path).text().then((text) => Bun.TOML.parse(text) as ClusterConfig);
}

export const schedulerUrl = (cfg: ClusterConfig): string =>
  `http://${cfg.hosts[cfg.scheduler.host]!.address}:${cfg.sccache_dist.scheduler_port}`;

export const serverAddress = (cfg: ClusterConfig, host: string): string =>
  `${cfg.hosts[host]!.address}:${cfg.sccache_dist.server_port}`;

export const serverHosts = (cfg: ClusterConfig): string[] =>
  Object.entries(cfg.hosts)
    .filter(([, h]) => h.roles.includes("server"))
    .map(([name]) => name);

export function renderScheduler(cfg: ClusterConfig, secrets: Secrets): string {
  const hub = cfg.hosts[cfg.scheduler.host]!;
  return [
    `public_addr = "${hub.address}:${cfg.sccache_dist.scheduler_port}"`,
    "",
    "[client_auth]",
    'type = "token"',
    `token = "${secrets.clientToken}"`,
    "",
    "[server_auth]",
    'type = "jwt_hs256"',
    `secret_key = "${secrets.hs256Key}"`,
    "",
  ].join("\n");
}

export function renderServer(cfg: ClusterConfig, host: string, secrets: Secrets): string {
  const h = cfg.hosts[host]!;
  return [
    `cache_dir = "${h.work}/cache/sccache-dist-toolchains"`,
    `public_addr = "${serverAddress(cfg, host)}"`,
    `scheduler_url = "${schedulerUrl(cfg)}"`,
    "",
    "[builder]",
    'type = "overlay"',
    `build_dir = "${h.work}/tmp/sccache-dist-build"`,
    'bwrap_path = "/usr/bin/bwrap"',
    "",
    "[scheduler_auth]",
    'type = "jwt_token"',
    `token = "${secrets.serverTokens[host]}"`,
    "",
  ].join("\n");
}

export function renderClient(cfg: ClusterConfig, host: string, secrets: Secrets): string {
  const h = cfg.hosts[host]!;
  return [
    "[dist]",
    `scheduler_url = "${schedulerUrl(cfg)}"`,
    `cache_dir = "${h.work}/cache/sccache-dist-client"`,
    "",
    "[dist.auth]",
    'type = "token"',
    `token = "${secrets.clientToken}"`,
    "",
    "[cache.disk]",
    `dir = "${h.work}/cache/sccache"`,
    "size = 107374182400",
    "",
  ].join("\n");
}

export function renderUnit(kind: "scheduler" | "server", cfg: ClusterConfig, host: string, binary: string): string {
  const h = cfg.hosts[host]!;
  const lines = [
    "[Unit]",
    `Description=sccache-dist ${kind} (cluster/build.toml)`,
    "After=network-online.target",
    "Wants=network-online.target",
    "",
    "[Service]",
    // The scheduler detaches from its launcher after binding its port (systemd follows the survivor); the server stays in the foreground.
    `Type=${kind === "scheduler" ? "forking" : "simple"}`,
    `ExecStart=${binary} ${kind} --config /etc/sccache-dist/${kind}.toml`,
    "Restart=always",
    "RestartSec=3",
  ];
  // The overlay builder needs root; the slice keeps production healthy on shared hosts.
  if (kind === "server" && h.slice) lines.push(`Slice=${h.slice}`, "Nice=19", "IOSchedulingClass=idle");
  lines.push("", "[Install]", "WantedBy=multi-user.target", "");
  return lines.join("\n");
}

export function redact(text: string): string {
  return text.replace(/^(token|secret_key) = ".*"$/gm, '$1 = "<redacted>"');
}

async function ssh(host: string, script: string, stdin?: string): Promise<string> {
  const proc = Bun.spawn(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15", host, "bash -s"], {
    stdin: new Response(`set -o pipefail\n${script}\n`),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  if (code !== 0) throw new Error(`ssh ${host} exit ${code} [${script.replace(/'[A-Za-z0-9+/=]{40,}'/g, "'<data>'").slice(0, 100)}]: ${err.trim() || out.trim()}`);
  return out.trim();
}

async function putFile(host: string, path: string, mode: string, content: string): Promise<void> {
  const b64 = Buffer.from(content).toString("base64");
  await ssh(host, `sudo mkdir -p "$(dirname '${path}')"; echo '${b64}' | base64 -d | sudo tee '${path}' >/dev/null; sudo chmod ${mode} '${path}'`);
}

async function loadSecrets(cfg: ClusterConfig): Promise<Secrets> {
  const hub = cfg.hosts[cfg.scheduler.host]!;
  const hubBin = await binaryPath(cfg, cfg.scheduler.host);
  const existing = await ssh(hub.ssh, "sudo cat /etc/sccache-dist/secrets.json 2>/dev/null || true");
  const parsed: Secrets = existing ? JSON.parse(existing) : { clientToken: "", hs256Key: "", serverTokens: {} };
  const present = (value: string) => value.length >= 16;
  if (!present(parsed.clientToken)) parsed.clientToken = await ssh(hub.ssh, `${hubBin} auth generate-shared-token | tail -1`);
  if (!present(parsed.hs256Key)) parsed.hs256Key = await ssh(hub.ssh, `${hubBin} auth generate-jwt-hs256-key | tail -1`);
  for (const host of serverHosts(cfg)) {
    if (present(parsed.serverTokens[host] ?? "")) continue;
    const addr = serverAddress(cfg, host);
    parsed.serverTokens[host] = await ssh(
      hub.ssh,
      `${hubBin} auth generate-jwt-hs256-server-token --server ${addr} --secret-key '${parsed.hs256Key}' | tail -1`,
    );
  }
  await putFile(hub.ssh, "/etc/sccache-dist/secrets.json", "600", JSON.stringify(parsed));
  return parsed;
}

async function binaryPath(cfg: ClusterConfig, host: string): Promise<string> {
  const h = cfg.hosts[host]!;
  const built = posix.join(h.work, cfg.sccache_dist.tools_dir, "bin/sccache-dist");
  return ssh(h.ssh, `[ -x '${built}' ] && echo '${built}' || command -v sccache-dist`);
}

export async function apply(cfg: ClusterConfig): Promise<void> {
  const secrets = await loadSecrets(cfg);
  const hub = cfg.scheduler.host;
  const hubBin = await binaryPath(cfg, hub);
  await putFile(cfg.hosts[hub]!.ssh, "/etc/sccache-dist/scheduler.toml", "600", renderScheduler(cfg, secrets));
  await putFile(cfg.hosts[hub]!.ssh, "/etc/systemd/system/sccache-dist-scheduler.service", "644", renderUnit("scheduler", cfg, hub, hubBin));
  await ssh(cfg.hosts[hub]!.ssh, "sudo systemctl daemon-reload && sudo systemctl enable sccache-dist-scheduler && sudo systemctl restart sccache-dist-scheduler");
  for (const host of serverHosts(cfg)) {
    const h = cfg.hosts[host]!;
    const bin = await binaryPath(cfg, host);
    await ssh(h.ssh, `sudo install -d -m 755 '${h.work}/cache/sccache-dist-toolchains' '${h.work}/tmp/sccache-dist-build'`);
    await putFile(h.ssh, "/etc/sccache-dist/server.toml", "600", renderServer(cfg, host, secrets));
    await putFile(h.ssh, "/etc/systemd/system/sccache-dist-server.service", "644", renderUnit("server", cfg, host, bin));
    await ssh(h.ssh, "sudo systemctl daemon-reload && sudo systemctl enable sccache-dist-server && sudo systemctl restart sccache-dist-server");
    await putFile(h.ssh, `${h.work}/sccache-dist-client.toml`, "600", renderClient(cfg, host, secrets));
    await ssh(h.ssh, `sudo chown ${h.user} '${h.work}/sccache-dist-client.toml'`);
  }
}

async function status(cfg: ClusterConfig): Promise<void> {
  const hub = cfg.hosts[cfg.scheduler.host]!;
  const url = schedulerUrl(cfg);
  console.log(await ssh(hub.ssh, `curl -s -m5 ${url}/api/v1/scheduler/status || echo unreachable`));
  for (const host of serverHosts(cfg)) {
    console.log(host, await ssh(cfg.hosts[host]!.ssh, "systemctl is-active sccache-dist-server || true"));
  }
}

if (import.meta.main) {
  const [command = "plan"] = Bun.argv.slice(2);
  const cfg = await loadConfig();
  if (command === "apply") await apply(cfg);
  else if (command === "status") await status(cfg);
  else {
    const dummy: Secrets = { clientToken: "x", hs256Key: "x", serverTokens: Object.fromEntries(serverHosts(cfg).map((h) => [h, "x"])) };
    console.log(redact(renderScheduler(cfg, dummy)));
    for (const host of serverHosts(cfg)) console.log(`# ${host}\n${redact(renderServer(cfg, host, dummy))}`);
  }
}
