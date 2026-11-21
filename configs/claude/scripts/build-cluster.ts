#!/usr/bin/env bun
// Build cluster as code: equal nodes, one declaration (cluster/build.toml), no node has a role by name.
//   bun build-cluster.ts plan                    print the rendered configs (secrets redacted)
//   bun build-cluster.ts apply                   secrets, configs, slice and units on every node; restarts only what changed
//   bun build-cluster.ts status                  scheduler status and per-node unit state
//   bun build-cluster.ts ssh                     add the node names as aliases next to the existing ones in the PC ssh config
//   bun build-cluster.ts hostnames               set each node's hostname to its name, write the mesh DNS block on every node
//   bun build-cluster.ts linkers                 build mold and wild on every node for its own ISA, install under /usr/local/bin
//   bun build-cluster.ts runners                 add the GitHub Actions runners a node lacks: ceil(vcpu / cores_per_runner) each
//   bun build-cluster.ts measure                 live load, free memory and free disk per node
//   bun build-cluster.ts place [--mem G] [--disk G]            the node a new long job should go to
//   bun build-cluster.ts run [--mem G] [--disk G] -- <cmd>     place, then run <cmd> on that node in its work dir
//   bun build-cluster.ts batch [--jobs N] [--cores C] [--mem G] [--pin node]   the same local-compile job set, placed or pinned
//   bun build-cluster.ts probe <node>            cold multi-crate cargo build through sccache-dist from <node>
//   bun build-cluster.ts failover                stop each unit in turn, prove the cluster still builds, restore
//   --config <file> works with every command (an alternate declaration, for example another scheduler host).
// Every name, address, port and path comes from cluster/build.toml. Secrets are generated on first apply, kept in
// /etc/sccache-dist/secrets.json (root, 0600) on the scheduler node and never printed.
import { homedir } from "node:os";
import { join, posix, resolve } from "node:path";

export type Host = {
  index: number;
  aliases: string[];
  ssh: string;
  address: string;
  user: string;
  work: string;
  vcpu: number;
  target_cpu: string;
  roles: string[];
};
export type ClusterConfig = {
  cluster: { name: string; node_prefix: string; mesh_domain: string };
  mesh: { cidr: string };
  scheduler: { host: string };
  policy: { build_slice: string; cpu_weight: number; io_weight: number; memory_high_percent: number; memory_max_percent: number };
  runners: { org: string; cores_per_runner: number; labels: string[]; dir_prefix: string };
  sccache_dist: { scheduler_port: number; server_port: number; version: string; build_target_cpu: string; tools_dir: string };
  nativelink: { scheduler: string; scheduler_port: number; cas_port: number };
  linkers: { mold: string; wild: string; tools_dir: string };
  bench: { repo: string; package: string; profile_lto: string; codegen_units: number };
  hosts: Record<string, Host>;
};
export type Secrets = { clientToken: string; hs256Key: string; serverTokens: Record<string, string> };

const root = resolve(import.meta.dir, "..", "..", "..");
const SSH_OPTIONS = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=15"];

export async function loadConfig(path = join(root, "cluster", "build.toml")): Promise<ClusterConfig> {
  const cfg = Bun.TOML.parse(await Bun.file(path).text()) as ClusterConfig;
  for (const [name, host] of Object.entries(cfg.hosts)) {
    const derived = nodeName(cfg, host);
    if (name !== derived) throw new Error(`cluster/build.toml: [hosts.${name}] has index ${host.index}, its name must be ${derived}`);
  }
  if (!cfg.hosts[cfg.scheduler.host]) throw new Error(`scheduler host ${cfg.scheduler.host} is not a declared node`);
  return cfg;
}

export const nodeName = (cfg: ClusterConfig, host: Host): string => `${cfg.cluster.node_prefix}${host.index}`;
export const nodeNames = (cfg: ClusterConfig): string[] => Object.keys(cfg.hosts);
export const meshName = (cfg: ClusterConfig, node: string): string => `${node}.${cfg.cluster.mesh_domain}`;

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
  // The overlay builder needs root; the same slice on every node keeps interactive work and CI responsive during a burst.
  if (kind === "server") lines.push(`Slice=${cfg.policy.build_slice}`, "Nice=19", "IOSchedulingClass=idle");
  lines.push("", "[Install]", "WantedBy=multi-user.target", "");
  return lines.join("\n");
}

export function renderSlice(cfg: ClusterConfig, memTotalBytes: number): string {
  const gib = (percent: number) => `${Math.floor((memTotalBytes * percent) / 100 / 2 ** 30)}G`;
  return [
    "[Unit]",
    `Description=Build bursts of cluster ${cfg.cluster.name} (cluster/build.toml [policy])`,
    "",
    "[Slice]",
    `CPUWeight=${cfg.policy.cpu_weight}`,
    `IOWeight=${cfg.policy.io_weight}`,
    `MemoryHigh=${gib(cfg.policy.memory_high_percent)}`,
    `MemoryMax=${gib(cfg.policy.memory_max_percent)}`,
    "",
  ].join("\n");
}

/** The mesh DNS block of /etc/hosts: one line per declared node, any other line already in the block is kept. */
export function renderMeshBlock(cfg: ClusterConfig, existing: string[]): string[] {
  const declared = new Map<string, string>();
  for (const [name, h] of Object.entries(cfg.hosts)) {
    declared.set(h.address, [meshName(cfg, name), ...h.aliases.map((alias) => meshName(cfg, alias))].join(" "));
  }
  const kept = existing.filter((line) => line.trim() && !line.startsWith("#") && !declared.has(line.trim().split(/\s+/)[0] ?? ""));
  return [...[...declared].map(([address, names]) => `${address} ${names}`), ...kept];
}

/** Adds `name` to every `Host` and `Match host` line that already lists one of `known`; returns the new text and how many lines changed. */
export function addSshAlias(text: string, name: string, known: string[]): { text: string; changed: number } {
  let changed = 0;
  const lines = text.split("\n").map((line) => {
    const host = /^(\s*Host\s+)(.*?)(\s*)$/.exec(line);
    if (host) {
      const names = host[2]!.split(/\s+/);
      if (names.some((n) => known.includes(n)) && !names.includes(name)) {
        changed++;
        return `${host[1]}${[...names, name].join(" ")}${host[3]}`;
      }
      return line;
    }
    const match = /^(\s*Match\s+host\s+)([^\s]+)(.*)$/.exec(line);
    if (match) {
      const names = match[2]!.split(",");
      if (names.some((n) => known.includes(n)) && !names.includes(name)) {
        changed++;
        return `${match[1]}${[...names, name].join(",")}${match[3]}`;
      }
    }
    return line;
  });
  return { text: lines.join("\n"), changed };
}

export type Measure = { node: string; vcpu: number; load1: number; load5: number; memAvailGiB: number; diskFreeGiB: number };

/** Free cores = cores minus the blended load; a node is eligible when it has the memory and disk the job asks for. */
export function freeCores(m: Measure): number {
  return m.vcpu - (0.5 * m.load1 + 0.5 * m.load5);
}

export type Reserved = Record<string, { cores: number; memGiB: number }>;

/** `reserved` holds what jobs already placed by this scheduler will take but the node's load average has not shown yet. */
export function place(
  measures: Measure[],
  need: { memGiB?: number; diskGiB?: number } = {},
  reserved: Reserved = {},
): { node: string; ranked: { node: string; freeCores: number }[] } {
  const held = (m: Measure): Measure => ({
    ...m,
    load1: m.load1 + (reserved[m.node]?.cores ?? 0),
    load5: m.load5 + (reserved[m.node]?.cores ?? 0),
    memAvailGiB: m.memAvailGiB - (reserved[m.node]?.memGiB ?? 0),
  });
  const eligible = measures.map(held).filter((m) => m.memAvailGiB >= (need.memGiB ?? 0) && m.diskFreeGiB >= (need.diskGiB ?? 0));
  if (!eligible.length) throw new Error("no node has the memory and disk the job asks for: burst to Cloudflare Containers or gcloud");
  const ranked = eligible
    .map((m) => ({ node: m.node, freeCores: Math.round(freeCores(m) * 100) / 100, memAvailGiB: m.memAvailGiB }))
    .sort((a, b) => b.freeCores - a.freeCores || b.memAvailGiB - a.memAvailGiB);
  return { node: ranked[0]!.node, ranked: ranked.map(({ node, freeCores }) => ({ node, freeCores })) };
}

export function redact(text: string): string {
  return text.replace(/^(token|secret_key) = ".*"$/gm, '$1 = "<redacted>"');
}

async function ssh(host: string, script: string, secret?: string): Promise<string> {
  const proc = Bun.spawn(["ssh", ...SSH_OPTIONS, host, "bash -s"], {
    stdin: new Response(`set -o pipefail\n${script}\n`),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  const mask = (text: string) => (secret ? text.replaceAll(secret, "<secret>") : text);
  if (code !== 0) throw new Error(mask(`ssh ${host} exit ${code} [${script.replace(/'[A-Za-z0-9+/=]{40,}'/g, "'<data>'").slice(0, 100)}]: ${err.trim() || out.trim()}`));
  return out.trim();
}

/** Writes the file when its content differs; returns whether it changed. */
async function putFile(host: string, path: string, mode: string, content: string): Promise<boolean> {
  const b64 = Buffer.from(content).toString("base64");
  const result = await ssh(
    host,
    `sudo mkdir -p "$(dirname '${path}')"; new=$(mktemp); echo '${b64}' | base64 -d > "$new"; if sudo cmp -s "$new" '${path}'; then echo same; else sudo install -m ${mode} "$new" '${path}'; echo changed; fi; rm -f "$new"`,
  );
  return result === "changed";
}

async function loadSecrets(cfg: ClusterConfig): Promise<Secrets> {
  const hub = cfg.hosts[cfg.scheduler.host]!;
  const hubBin = await binaryPath(cfg, cfg.scheduler.host);
  const existing = await ssh(hub.ssh, "sudo cat /etc/sccache-dist/secrets.json 2>/dev/null || true");
  const parsed: Secrets = existing ? JSON.parse(existing) : { clientToken: "", hs256Key: "", serverTokens: {} };
  const before = JSON.stringify(parsed);
  const present = (value: string) => value.length >= 16;
  if (!present(parsed.clientToken)) parsed.clientToken = await ssh(hub.ssh, `${hubBin} auth generate-shared-token | tail -1`);
  if (!present(parsed.hs256Key)) parsed.hs256Key = await ssh(hub.ssh, `${hubBin} auth generate-jwt-hs256-key | tail -1`);
  for (const host of serverHosts(cfg)) {
    if (present(parsed.serverTokens[host] ?? "")) continue;
    parsed.serverTokens[host] = await ssh(
      hub.ssh,
      `${hubBin} auth generate-jwt-hs256-server-token --server ${serverAddress(cfg, host)} --secret-key '${parsed.hs256Key}' | tail -1`,
    );
  }
  if (JSON.stringify(parsed) !== before) await putFile(hub.ssh, "/etc/sccache-dist/secrets.json", "600", JSON.stringify(parsed));
  return parsed;
}

// Every node holds the same secrets file, so the scheduler can move to any node by editing [scheduler] and applying.
async function binaryPath(cfg: ClusterConfig, host: string): Promise<string> {
  const h = cfg.hosts[host]!;
  const built = posix.join(h.work, cfg.sccache_dist.tools_dir, "bin/sccache-dist");
  return ssh(h.ssh, `[ -x '${built}' ] && echo '${built}' || command -v sccache-dist`);
}

export async function apply(cfg: ClusterConfig): Promise<void> {
  const hub = cfg.scheduler.host;
  const secrets = await loadSecrets(cfg);
  await Promise.all(
    nodeNames(cfg).map(async (host) => {
      const h = cfg.hosts[host]!;
      const bin = await binaryPath(cfg, host);
      const unitsChanged: string[] = [];
      const memTotal = Number(await ssh(h.ssh, "awk '/MemTotal/ {print $2 * 1024}' /proc/meminfo"));
      const sliceChanged = await putFile(h.ssh, `/etc/systemd/system/${cfg.policy.build_slice}`, "644", renderSlice(cfg, memTotal));
      if (host !== hub) await putFile(h.ssh, "/etc/sccache-dist/secrets.json", "600", JSON.stringify(secrets));
      if (host === hub) {
        const changed = [
          await putFile(h.ssh, "/etc/sccache-dist/scheduler.toml", "600", renderScheduler(cfg, secrets)),
          await putFile(h.ssh, "/etc/systemd/system/sccache-dist-scheduler.service", "644", renderUnit("scheduler", cfg, host, bin)),
        ];
        if (changed.some(Boolean)) unitsChanged.push("sccache-dist-scheduler");
        await ssh(h.ssh, "sudo systemctl daemon-reload && sudo systemctl enable sccache-dist-scheduler >/dev/null 2>&1; sudo systemctl is-active --quiet sccache-dist-scheduler || sudo systemctl start sccache-dist-scheduler");
      } else {
        // A node that is not the scheduler must not run one: this is what moving the scheduler means.
        await ssh(h.ssh, "if systemctl cat sccache-dist-scheduler >/dev/null 2>&1; then sudo systemctl disable --now sccache-dist-scheduler; fi");
      }
      if (h.roles.includes("server")) {
        await ssh(h.ssh, `sudo install -d -m 755 '${h.work}/cache/sccache-dist-toolchains' '${h.work}/tmp/sccache-dist-build'`);
        const changed = [
          await putFile(h.ssh, "/etc/sccache-dist/server.toml", "600", renderServer(cfg, host, secrets)),
          await putFile(h.ssh, "/etc/systemd/system/sccache-dist-server.service", "644", renderUnit("server", cfg, host, bin)),
          sliceChanged,
        ];
        if (changed.some(Boolean)) unitsChanged.push("sccache-dist-server");
        await ssh(h.ssh, "sudo systemctl daemon-reload && sudo systemctl enable sccache-dist-server >/dev/null 2>&1; sudo systemctl is-active --quiet sccache-dist-server || sudo systemctl start sccache-dist-server");
      }
      await putFile(h.ssh, `${h.work}/sccache-dist-client.toml`, "600", renderClient(cfg, host, secrets));
      await ssh(h.ssh, `sudo chown ${h.user} '${h.work}/sccache-dist-client.toml'`);
      if (unitsChanged.length) await ssh(h.ssh, `sudo systemctl restart ${unitsChanged.join(" ")}`);
      console.log(host, unitsChanged.length ? `restarted ${unitsChanged.join(", ")}` : "up to date");
    }),
  );
}

/** The scheduler answers only authenticated clients, so its status is read through sccache with the node's client config. */
async function distStatus(cfg: ClusterConfig, node: string): Promise<string> {
  const h = cfg.hosts[node]!;
  return ssh(
    h.ssh,
    `export SCCACHE_CONF='${h.work}/sccache-dist-client.toml' SCCACHE_SERVER_PORT=4999; sccache --dist-status | head -1; sccache --stop-server >/dev/null 2>&1 || true`,
  ).catch(() => "unreachable");
}

export const runnersWanted = (cfg: ClusterConfig, node: string): number => Math.ceil(cfg.hosts[node]!.vcpu / cfg.runners.cores_per_runner);

export function runnerScript(cfg: ClusterConfig, node: string, index: number, token: string): string {
  const h = cfg.hosts[node]!;
  const base = posix.join(h.work, "runners");
  const dir = `${base}/${cfg.runners.dir_prefix}${index}`;
  const name = `${node}-${cfg.runners.dir_prefix}${index}`;
  const unit = `actions.runner.${cfg.runners.org}.${name}.service`;
  const labels = [...cfg.runners.labels, node].join(",");
  // The registration token arrives on stdin and goes to config.sh through the environment, never through argv.
  return [
    "set -e",
    "read -r token",
    token,
    `template=$(ls -d '${base}'/${cfg.runners.dir_prefix}*/ | head -1); dir='${dir}'`,
    `[ ! -e "$dir/.runner" ] || { echo exists; exit 0; }`,
    `sudo mkdir -p "$dir/home"`,
    `(cd "$template" && sudo tar cf - --exclude=./_work --exclude=./_diag --exclude=./home --exclude=./.credentials --exclude=./.credentials_rsaparams --exclude='./.runner*' --exclude=./.service --exclude=./.env --exclude=./.path .) | sudo tar xf - -C "$dir"`,
    `sudo chown -R ${h.user}: "$dir"`,
    `export ACTIONS_RUNNER_INPUT_URL='https://github.com/${cfg.runners.org}' ACTIONS_RUNNER_INPUT_TOKEN="$token" ACTIONS_RUNNER_INPUT_NAME='${name}' ACTIONS_RUNNER_INPUT_LABELS='${labels}' ACTIONS_RUNNER_INPUT_WORK=_work ACTIONS_RUNNER_INPUT_REPLACE=true`,
    `cd "$dir" && sudo --preserve-env=ACTIONS_RUNNER_INPUT_URL,ACTIONS_RUNNER_INPUT_TOKEN,ACTIONS_RUNNER_INPUT_NAME,ACTIONS_RUNNER_INPUT_LABELS,ACTIONS_RUNNER_INPUT_WORK,ACTIONS_RUNNER_INPUT_REPLACE -u ${h.user} ./config.sh --unattended >/dev/null`,
    `sudo ./svc.sh install ${h.user} >/dev/null`,
    `sudo mkdir -p /etc/systemd/system/${unit}.d`,
    `printf '[Service]\nEnvironment=HOME=%s/home\nEnvironment=CARGO_HOME=%s/home/.cargo\nEnvironment=RUSTUP_HOME=%s/home/.rustup\nEnvironment=BUN_INSTALL=%s/home/.bun\nEnvironment=XDG_CACHE_HOME=%s/home/.cache\nEnvironment=PATH=%s/home/.cargo/bin:%s/home/.bun/bin:/usr/local/sbin:/usr/local/bin:/usr/bin:/bin\n' "$dir" "$dir" "$dir" "$dir" "$dir" "$dir" "$dir" | sudo tee /etc/systemd/system/${unit}.d/isolate.conf >/dev/null`,
    `sudo systemctl daemon-reload; sudo ./svc.sh start >/dev/null; systemctl is-active ${unit}`,
  ].join("\n");
}

async function runners(cfg: ClusterConfig): Promise<void> {
  for (const [node, h] of Object.entries(cfg.hosts)) {
    const existing = Number(await ssh(h.ssh, `ls -d '${posix.join(h.work, "runners")}'/${cfg.runners.dir_prefix}*/.runner 2>/dev/null | wc -l`));
    for (let index = existing + 1; index <= runnersWanted(cfg, node); index++) {
      const registration = Bun.spawn(["gh", "api", "-X", "POST", `orgs/${cfg.runners.org}/actions/runners/registration-token`, "--jq", ".token"], { stdout: "pipe" });
      const token = (await new Response(registration.stdout).text()).trim();
      const out = await ssh(h.ssh, runnerScript(cfg, node, index, token), token);
      console.log(node, `${cfg.runners.dir_prefix}${index}`, out);
    }
    if (existing >= runnersWanted(cfg, node)) console.log(node, `${existing} runners, wanted ${runnersWanted(cfg, node)}`);
  }
}

async function status(cfg: ClusterConfig): Promise<void> {
  console.log(cfg.scheduler.host, "scheduler", await distStatus(cfg, cfg.scheduler.host));
  for (const host of serverHosts(cfg)) {
    console.log(host, "server", await ssh(cfg.hosts[host]!.ssh, "systemctl is-active sccache-dist-server || true"));
  }
}

async function sshAliases(cfg: ClusterConfig): Promise<void> {
  const files = [join(homedir(), ".ssh", "config"), join(homedir(), ".ssh", "fleet", "ssh_config")];
  for (const path of files) {
    const file = Bun.file(path);
    if (!(await file.exists())) continue;
    let text = await file.text();
    let total = 0;
    for (const [name, h] of Object.entries(cfg.hosts)) {
      const result = addSshAlias(text, name, [...h.aliases, name]);
      text = result.text;
      total += result.changed;
    }
    if (total) await Bun.write(path, text);
    console.log(path, `${total} lines updated`);
  }
}

async function hostnames(cfg: ClusterConfig): Promise<void> {
  await Promise.all(
    Object.entries(cfg.hosts).map(async ([name, h]) => {
      // /etc/hosts first, so sudo keeps resolving the new name; cloud-init must not put the provider name back.
      const script = [
        `grep -qE '^127\\.0\\.1\\.1[[:space:]]+${name}$' /etc/hosts || echo '127.0.1.1 ${name}' | sudo tee -a /etc/hosts >/dev/null`,
        `[ "$(hostnamectl --static)" = '${name}' ] || sudo hostnamectl set-hostname '${name}'`,
        `if [ -d /etc/cloud/cloud.cfg.d ]; then echo 'preserve_hostname: true' | sudo tee /etc/cloud/cloud.cfg.d/99-fw-hostname.cfg >/dev/null; fi`,
        "sudo cat /etc/hosts",
      ].join("\n");
      const hosts = (await ssh(h.ssh, script)).split("\n");
      const begin = hosts.findIndex((l) => l === "# aphrody-mesh begin");
      const end = hosts.findIndex((l) => l === "# aphrody-mesh end");
      const inside = begin >= 0 && end > begin ? hosts.slice(begin + 1, end) : [];
      const block = ["# aphrody-mesh begin", ...renderMeshBlock(cfg, inside), "# aphrody-mesh end"].join("\n");
      const outside = begin >= 0 && end > begin ? [...hosts.slice(0, begin), ...hosts.slice(end + 1)] : hosts;
      await putFile(h.ssh, "/etc/hosts", "644", `${outside.join("\n").trimEnd()}\n\n${block}\n`);
      console.log(name, await ssh(h.ssh, "hostnamectl --static; getent hosts n1.mesh n2.mesh | tr '\\n' ' '"));
    }),
  );
}

async function rustChannel(): Promise<string> {
  const toolchain = Bun.TOML.parse(await Bun.file(join(root, "rust-toolchain.toml")).text()) as { toolchain: { channel: string } };
  return toolchain.toolchain.channel;
}

async function linkers(cfg: ClusterConfig): Promise<void> {
  const channel = await rustChannel();
  await Promise.all(
    Object.entries(cfg.hosts).map(async ([name, h]) => {
      const dir = posix.join(h.work, cfg.linkers.tools_dir);
      const started = Date.now();
      // Built on the node for its own ISA; sccache is bypassed so no object compiled for the other tier is reused.
      const script = [
        "set -e",
        `d='${dir}'; mkdir -p "$d/src" "$d/bin"`,
        "unset RUSTC_WRAPPER SCCACHE_DIR",
        `export RUSTFLAGS='-C target-cpu=${h.target_cpu}' CARGO_TARGET_DIR="$d/target"`,
        `[ -d "$d/src/mold" ] || git clone -q --depth 1 --branch v${cfg.linkers.mold} https://github.com/rui314/mold "$d/src/mold"`,
        `(cd "$d/src/mold" && cargo +${channel} build --release --locked --package mold-cli && install -m 755 "$d/target/release/mold" "$d/bin/mold")`,
        `cargo +${channel} install --locked --root "$d/wild-root" --force wild-linker --version ${cfg.linkers.wild}`,
        `install -m 755 "$d/wild-root/bin/wild" "$d/bin/wild"`,
        `sudo install -m 755 "$d/bin/mold" /usr/local/bin/mold; sudo install -m 755 "$d/bin/wild" /usr/local/bin/wild`,
        "/usr/local/bin/mold --version; /usr/local/bin/wild --version",
      ].join("\n");
      const out = await ssh(h.ssh, script);
      console.log(name, `${Math.round((Date.now() - started) / 1000)}s`, out.split("\n").slice(-2).join(" | "));
    }),
  );
}

export async function measure(cfg: ClusterConfig, node: string): Promise<Measure> {
  const h = cfg.hosts[node]!;
  const out = await ssh(h.ssh, `read l1 l5 _ < /proc/loadavg; echo "$l1 $l5 $(awk '/MemAvailable/ {print $2}' /proc/meminfo) $(df -Pk '${h.work}' | awk 'NR==2 {print $4}')"`);
  const [load1, load5, memKiB, diskKiB] = out.split(" ").map(Number) as [number, number, number, number];
  const gib = (kib: number) => Math.round((kib / 2 ** 20) * 10) / 10;
  return { node, vcpu: h.vcpu, load1, load5, memAvailGiB: gib(memKiB), diskFreeGiB: gib(diskKiB) };
}

const measureAll = (cfg: ClusterConfig) => Promise.all(nodeNames(cfg).map((node) => measure(cfg, node)));

function flag(args: string[], name: string): number | undefined {
  const i = args.indexOf(name);
  return i >= 0 ? Number(args[i + 1]) : undefined;
}

// Probe workspace: independent library crates, so cargo hands sccache-dist many rustc invocations at once.
// The nonce makes every run a cold compile; the binary prints the nonce, which is how the result is checked by content.
export const PROBE_CRATES = 24;
export function probeScript(cfg: ClusterConfig, node: string, nonce: string, options: { dist?: boolean; jobs?: number } = {}): string {
  const dist = options.dist ?? true;
  const jobs = options.jobs ?? PROBE_CRATES;
  const h = cfg.hosts[node]!;
  const body = (i: number) => Array.from({ length: 400 }, (_, j) => `pub fn f${j}(x: u64) -> u64 { x.wrapping_mul(${i * 1000 + j + 3}).rotate_left(${(j % 31) + 1}) ^ ${nonce.length + j} }`).join("\n");
  const members = Array.from({ length: PROBE_CRATES }, (_, i) => `c${i}`);
  return [
    "set -e",
    `dir=$(mktemp -d '${h.work}/tmp/probe.XXXXXX'); cd "$dir"; trap 'rm -rf "$dir"' EXIT`,
    `printf '[workspace]\\nmembers = [${members.map((m) => `"${m}"`).join(", ")}, "app"]\\nresolver = "2"\\n[profile.dev]\\nincremental = false\\n' > Cargo.toml`,
    ...members.map((m, i) => `mkdir -p ${m}/src; printf '[package]\\nname = "${m}"\\nversion = "0.0.0"\\nedition = "2021"\\n' > ${m}/Cargo.toml; cat > ${m}/src/lib.rs <<'EOF'\n// ${nonce}\n${body(i)}\nEOF`),
    `mkdir -p app/src; { printf '[package]\\nname = "app"\\nversion = "0.0.0"\\nedition = "2021"\\n[dependencies]\\n'; ${members.map((m) => `printf '${m} = { path = "../${m}" }\\n'`).join("; ")}; } > app/Cargo.toml`,
    `{ echo 'fn main() { let mut s = 0u64;'; ${members.map((m) => `echo 's ^= ${m}::f1(${nonce.length});'`).join("; ")}; echo 'println!("${nonce} {}", s); }'; } > app/src/main.rs`,
    dist
      ? `export SCCACHE_CONF='${h.work}/sccache-dist-client.toml' SCCACHE_SERVER_PORT=4999 RUSTC_WRAPPER=sccache CARGO_TARGET_DIR="$dir/target" SCCACHE_ERROR_LOG="$dir/sccache.log"; sccache --stop-server >/dev/null 2>&1 || true`
      : 'unset RUSTC_WRAPPER; export CARGO_TARGET_DIR="$dir/target"',
    "start=$(date +%s%N)",
    `cargo build -j ${jobs} -q 2>&1 | tail -3`,
    "end=$(date +%s%N)",
    'echo "wall_ms=$(( (end - start) / 1000000 ))"',
    'echo "output=$("$dir/target/debug/app")"',
    ...(dist
      ? [
          "echo \"dist=$(sccache --show-stats --stats-format json | jq -c '.stats | {compile_requests, dist_compiles, dist_errors, cache_hits}')\"",
          "sccache --stop-server >/dev/null 2>&1 || true",
        ]
      : []),
  ].join("\n");
}

async function probe(cfg: ClusterConfig, node: string): Promise<{ wallSeconds: number; output: string; nonce: string; dist: string }> {
  const nonce = `probe-${Date.now().toString(36)}`;
  const out = await ssh(cfg.hosts[node]!.ssh, probeScript(cfg, node, nonce));
  const field = (key: string) => out.split("\n").find((l) => l.startsWith(`${key}=`))?.slice(key.length + 1) ?? "";
  return { wallSeconds: Number(field("wall_ms")) / 1000, output: field("output"), nonce, dist: field("dist") };
}

type JobResult = { job: number; node: string; wallSeconds: number; ok: boolean };

/**
 * The same job set either pinned to one node (`pin`) or placed one by one on live load, with the cores and memory of
 * jobs already placed held back until the load average shows them. Samples every node while the set runs.
 */
export async function batch(cfg: ClusterConfig, options: { jobs: number; cores: number; memGiB: number; pin?: string }) {
  const started = Date.now();
  const samples: Record<string, { load1: number[]; memAvailGiB: number[] }> = Object.fromEntries(nodeNames(cfg).map((n) => [n, { load1: [], memAvailGiB: [] }]));
  let running = true;
  const sampler = (async () => {
    while (running) {
      for (const m of await measureAll(cfg)) {
        samples[m.node]!.load1.push(m.load1);
        samples[m.node]!.memAvailGiB.push(m.memAvailGiB);
      }
      await new Promise((r) => setTimeout(r, 4000));
    }
  })();
  // One snapshot: the reservations carry what the jobs placed since will add, so the decision costs no extra ssh round trip per job.
  const snapshot = await measureAll(cfg);
  const reserved: Reserved = {};
  const runs: Promise<JobResult>[] = [];
  const placements: string[] = [];
  for (let job = 0; job < options.jobs; job++) {
    const node = options.pin ?? place(snapshot, { memGiB: options.memGiB }, reserved).node;
    reserved[node] = { cores: (reserved[node]?.cores ?? 0) + options.cores, memGiB: (reserved[node]?.memGiB ?? 0) + options.memGiB };
    placements.push(node);
    const jobStarted = Date.now();
    runs.push(
      ssh(cfg.hosts[node]!.ssh, probeScript(cfg, node, `job${job}-${jobStarted.toString(36)}`, { dist: false, jobs: options.cores })).then(
        (out) => ({ job, node, wallSeconds: (Date.now() - jobStarted) / 1000, ok: out.includes(`output=job${job}-`) }),
      ),
    );
  }
  const results = await Promise.all(runs);
  running = false;
  await sampler;
  const peak = (values: number[]) => Math.round(Math.max(0, ...values) * 100) / 100;
  const low = (values: number[]) => Math.round(Math.min(...values) * 10) / 10;
  return {
    mode: options.pin ? `pinned to ${options.pin}` : "placed on live load",
    totalWallSeconds: (Date.now() - started) / 1000,
    jobsPerNode: Object.fromEntries(nodeNames(cfg).map((n) => [n, placements.filter((p) => p === n).length])),
    peakLoad1: Object.fromEntries(nodeNames(cfg).map((n) => [n, peak(samples[n]!.load1)])),
    minMemAvailGiB: Object.fromEntries(nodeNames(cfg).map((n) => [n, low(samples[n]!.memAvailGiB)])),
    results,
  };
}

async function failover(cfg: ClusterConfig): Promise<void> {
  const hub = cfg.scheduler.host;
  const servers = serverHosts(cfg);
  const check = async (label: string, from: string) => {
    const result = await probe(cfg, from);
    const sane = result.output.startsWith(result.nonce);
    const sched = await distStatus(cfg, from);
    console.log(JSON.stringify({ step: label, from, buildOutputMatchesNonce: sane, wallSeconds: result.wallSeconds, dist: result.dist, scheduler: sched }));
    if (!sane) throw new Error(`failover step "${label}": the build did not produce its nonce (${result.output})`);
  };
  await check("baseline, all units up", hub);
  for (const lost of servers) {
    const survivor = nodeNames(cfg).find((n) => n !== lost)!;
    await ssh(cfg.hosts[lost]!.ssh, "sudo systemctl stop sccache-dist-server");
    try {
      await new Promise((r) => setTimeout(r, 12_000));
      await check(`server on ${lost} stopped`, survivor);
    } finally {
      await ssh(cfg.hosts[lost]!.ssh, "sudo systemctl start sccache-dist-server");
    }
  }
  await ssh(cfg.hosts[hub]!.ssh, "sudo systemctl stop sccache-dist-scheduler");
  try {
    await check(`scheduler on ${hub} stopped (clients fall back to local compiles)`, hub);
  } finally {
    await ssh(cfg.hosts[hub]!.ssh, "sudo systemctl start sccache-dist-scheduler");
  }
  await new Promise((r) => setTimeout(r, 10_000));
  await check("restored, all units up", hub);
}

if (import.meta.main) {
  const argv = Bun.argv.slice(2);
  const configIndex = argv.indexOf("--config");
  const configPath = configIndex >= 0 ? argv.splice(configIndex, 2)[1] : undefined;
  const [command = "plan", ...rest] = argv;
  const cfg = await loadConfig(configPath);
  if (command === "apply") await apply(cfg);
  else if (command === "status") await status(cfg);
  else if (command === "ssh") await sshAliases(cfg);
  else if (command === "hostnames") await hostnames(cfg);
  else if (command === "linkers") await linkers(cfg);
  else if (command === "runners") await runners(cfg);
  else if (command === "measure") console.log(JSON.stringify(await measureAll(cfg), null, 2));
  else if (command === "place") {
    const measures = await measureAll(cfg);
    console.log(JSON.stringify({ ...place(measures, { memGiB: flag(rest, "--mem"), diskGiB: flag(rest, "--disk") }), measures }, null, 2));
  } else if (command === "run") {
    const split = rest.indexOf("--");
    const job = rest.slice(split + 1).join(" ");
    if (split < 0 || !job) throw new Error("usage: run [--mem G] [--disk G] -- <command>");
    const { node } = place(await measureAll(cfg), { memGiB: flag(rest.slice(0, split), "--mem"), diskGiB: flag(rest.slice(0, split), "--disk") });
    console.error(`placed on ${node}`);
    const proc = Bun.spawn(["ssh", ...SSH_OPTIONS, cfg.hosts[node]!.ssh, `cd '${cfg.hosts[node]!.work}' && ${job}`], { stdout: "inherit", stderr: "inherit" });
    process.exit(await proc.exited);
  } else if (command === "batch") {
    const pin = rest.indexOf("--pin");
    const outcome = await batch(cfg, { jobs: flag(rest, "--jobs") ?? 6, cores: flag(rest, "--cores") ?? 4, memGiB: flag(rest, "--mem") ?? 2, pin: pin >= 0 ? rest[pin + 1] : undefined });
    console.log(JSON.stringify(outcome, null, 2));
  } else if (command === "probe") console.log(JSON.stringify(await probe(cfg, rest[0] ?? cfg.scheduler.host)));
  else if (command === "failover") await failover(cfg);
  else {
    const dummy: Secrets = { clientToken: "x", hs256Key: "x", serverTokens: Object.fromEntries(serverHosts(cfg).map((h) => [h, "x"])) };
    console.log(redact(renderScheduler(cfg, dummy)));
    for (const host of serverHosts(cfg)) console.log(`# ${host}\n${redact(renderServer(cfg, host, dummy))}`);
    console.log(`# slice (32 GiB node)\n${renderSlice(cfg, 32 * 2 ** 30)}`);
  }
}
