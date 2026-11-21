// What the machines and the agent learned, kept private. Probes of this machine and of the hosts named in .local/config.toml go to
// the store and to .local/telemetry; notes (gotchas, decisions, facts) are added by `note`; every document that states a measurement,
// a host or a lesson is generated from the store into .local/docs and .claude.local.md, never written by hand into the repository.
//   bun testing/acceptance.ts telemetry        probe this machine and every host of [hosts] in .local/config.toml
//   bun testing/acceptance.ts note "<text>" [--kind gotcha|decision|fact]
//   bun testing/acceptance.ts docs             generate .local/docs/*.md and .claude.local.md from the store
import { arch, cpus, freemem, hostname, loadavg, platform, release, totalmem } from "node:os";
import { join } from "node:path";
import { engines, local, report, root } from "../acceptance.ts";
import { localDir } from "./local.ts";
import { mkdirp, spawn } from "./sys.ts";
import type { Store } from "./store.ts";

export type Probe = Record<string, string | number | string[] | null>;

const TOOLS = [
  "bun",
  "git",
  "cargo",
  "rustc",
  "hyperfine",
  "obscura",
  "agent-browser",
  "google-chrome",
  "chromium",
  "valkey-server",
  "redis-server",
  "cloudflared",
  "wrangler",
  "gh",
  "ssh",
];

/** This machine, from live discovery. */
export async function probeLocal(): Promise<Probe> {
  const found = await engines([], true).catch(() => []);
  return {
    host: hostname(),
    os: `${platform()} ${release()} ${arch()}`,
    cores: navigator.hardwareConcurrency,
    cpu: cpus()[0]?.model ?? null,
    memory_mib: Math.round(totalmem() / 1048576),
    memory_free_mib: Math.round(freemem() / 1048576),
    load: loadavg()
      .map((l) => Math.round(l * 100) / 100)
      .join(" "),
    bun: `${Bun.version}+${Bun.revision.slice(0, 9)}`,
    tools: TOOLS.flatMap((t) => (Bun.which(t) ? [t] : [])),
    engines: found.map((e) => `${e.id} ${e.path}`),
  };
}

const script = `echo host=$(hostname); echo os="$(uname -sr)"; echo cores=$(nproc); echo memory_mib=$(free -m | awk '/Mem:/{print $2}');
echo memory_free_mib=$(free -m | awk '/Mem:/{print $7}'); echo load="$(cut -d' ' -f1-3 /proc/loadavg)"; echo bun="$(bun --version 2>/dev/null)";
for t in ${TOOLS.join(" ")}; do command -v $t >/dev/null 2>&1 && echo tool=$t; done`;

/** One host over ssh, batch mode, short timeout; an unreachable host is a result, not an error. */
export async function probeRemote(ssh: string): Promise<Probe> {
  const proc = spawn(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", ssh, "sh -s"], {
    stdin: new Response(script),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  if (code !== 0) return { error: err.trim().split("\n").at(-1) ?? `exit ${code}` };
  const probe: Probe = { tools: [] };
  for (const line of out.split("\n")) {
    const [key, ...value] = line.split("=");
    if (!value.length) continue;
    if (key === "tool") (probe.tools as string[]).push(value.join("="));
    else probe[key] = /^\d+$/.test(value.join("=")) ? Number(value.join("=")) : value.join("=");
  }
  return probe;
}

export async function telemetry(store: Store) {
  const dir = join(localDir(root), "telemetry");
  await mkdirp(dir);
  const probes: [string, Probe][] = [["local", await probeLocal()]];
  for (const [alias, host] of Object.entries(local.hosts ?? {}))
    probes.push([alias, { role: host.role, ...(await probeRemote(host.ssh)) }]);
  for (const [name, data] of probes) {
    store.saveTelemetry(name, data);
    await Bun.write(join(dir, `${name}.json`), JSON.stringify(data, null, 1) + "\n");
  }
  store.emit("telemetry", { hosts: probes.map(([n]) => n) });
  return probes;
}

// ---- documents --------------------------------------------------------------------------------------------------------------------------

const table = (head: string[], rows: unknown[][]) =>
  [
    `| ${head.join(" | ")} |`,
    `| ${head.map(() => "---").join(" | ")} |`,
    ...rows.map((r) => `| ${r.map((c) => String(c ?? "").replaceAll("|", "\\|")).join(" | ")} |`),
  ].join("\n");
const head = (title: string, note: string) =>
  `# ${title}\n\nGenerated ${new Date().toISOString()} by \`bun testing/acceptance.ts docs\`; private (git-ignored). ${note}\n\n`;

/** Chromium's top level, read live, against the top level of this repository. */
async function layoutComparison(): Promise<string> {
  const token = Bun.env.GH_TOKEN ?? Bun.env.GITHUB_TOKEN;
  const res = await fetch("https://api.github.com/repos/chromium/chromium/contents/", {
    headers: token ? { authorization: `Bearer ${token}` } : {},
    signal: AbortSignal.timeout(15000),
  });
  if (!res.ok) return head("Layout against Chromium", "") + `The listing could not be read (${res.status}).\n`;
  const theirs = ((await res.json()) as { name: string; type: string }[]).map((e) => e.name);
  const ours = new Set(
    (await Bun.$`git -C ${root} ls-files`.text())
      .split("\n")
      .map((p) => p.split("/")[0])
      .filter(Boolean),
  );
  const same = theirs.filter((n) => ours.has(n));
  return (
    head("Layout against Chromium", "The static mapping with the reasons is docs/chromium-layout.md.") +
    `${same.length} of ${theirs.length} top-level entries of chromium/chromium exist here: ${same.join(", ")}.\n\n` +
    table(
      ["Chromium entry", "here"],
      theirs.map((n) => [n, ours.has(n) ? "yes" : "no"]),
    ) +
    "\n"
  );
}

export async function docs(store: Store) {
  const dir = join(localDir(root), "docs");
  await mkdirp(dir);
  const write = (name: string, text: string) => Bun.write(join(dir, name), text);

  const lessons = store.lessons();
  await write(
    "lessons.md",
    head(
      "Where an engine fell short of the full one",
      "The router's data: a lesson disappears when the engine catches up; one run proves little.",
    ) +
      (lessons.length
        ? table(
            ["host", "engine", "kind", "runs", "since", "last", "detail"],
            lessons.map((l) => [l.host, l.engine, l.kind, l.runs, l.first_run, l.last_run, l.detail]),
          )
        : "Nothing learned yet.") +
      "\n",
  );

  const last = report(store);
  const bench = store.bench(400);
  const latest = [...new Map(bench.map((b) => [`${b.suite}\0${b.engine}\0${b.target}`, b])).values()];
  await write(
    "measurements.md",
    head(
      "Measurements",
      "From the store: the latest run per engine and the latest benchmark per suite, engine and target.",
    ) +
      (last
        ? `## Run ${last.stamp} on ${last.host} (${last.corpus.length} pages)\n\n` +
          table(
            ["engine", "version", "pass", "auth", "consent", "refused", "fail", "median load ms", "peak RSS MiB"],
            last.engines.map((e) => [
              e.id,
              e.version,
              e.counts.pass,
              e.counts.auth,
              e.counts.consent,
              e.counts.refused,
              e.counts.fail,
              e.median_ms,
              e.peak_rss_mib,
            ]),
          ) +
          "\n\n"
        : "No run recorded.\n\n") +
      (latest.length
        ? "## Benchmarks\n\n" +
          table(
            ["suite", "engine", "version", "target", "result", "runs", "tree RSS MiB", "host", "at"],
            latest.map((b) => [
              b.suite,
              b.engine,
              b.version,
              b.target,
              b.suite === "jetstream"
                ? (b.result.error ?? b.result.score)
                : `${(b.result.mean_s * 1000).toFixed(1)} ms ± ${((b.result.stddev_s ?? 0) * 1000).toFixed(1)}`,
              b.result.runs ?? "",
              b.result.tree_rss_mib ?? b.result.peak_rss_mib ?? "",
              b.host,
              b.at,
            ]),
          ) +
          "\n"
        : "No benchmark recorded.\n"),
  );

  const ups = store.upstreams(1);
  await write(
    "upstreams.md",
    head("Upstreams", "The latest snapshot of \`track\`.") +
      (ups.length
        ? table(
            ["name", "kind", "version", "version date", "head", "head date", "behind our pin"],
            ups.map((u) => [
              u.name,
              u.snapshot.kind,
              u.snapshot.version,
              u.snapshot.version_date?.slice(0, 10),
              u.snapshot.head?.slice(0, 10),
              u.snapshot.head_date?.slice(0, 10),
              u.snapshot.behind,
            ]),
          )
        : "Run \`track\`.") +
      "\n",
  );

  const hosts = store.telemetry();
  await write(
    "fleet.md",
    head("Machines", "Probes of this machine and of the hosts of .local/config.toml.") +
      (hosts.length
        ? table(
            ["host", "role", "os", "cores", "memory MiB", "load", "bun", "tools", "probed"],
            hosts.map((h) => [
              h.host,
              h.data.role ?? "",
              h.data.os,
              h.data.cores,
              h.data.memory_mib,
              h.data.load,
              h.data.bun,
              [h.data.tools].flat().join(" "),
              h.at,
            ]),
          )
        : "Run \`telemetry\`.") +
      "\n",
  );

  await write(
    "chromium-layout.md",
    await layoutComparison().catch((e: Error) => head("Layout against Chromium", "") + `Not available: ${e.message}\n`),
  );

  const notes = store.notes();
  const byKind = Map.groupBy(notes, (n) => n.kind);
  const text =
    '# Learned here (private, generated)\n\nGenerated by `bun testing/acceptance.ts docs` from the store. Do not edit: add a note with `bun testing/acceptance.ts note "<text>" --kind gotcha`.\n\n' +
    [...byKind].map(([kind, list]) => `## ${kind}\n\n${list.map((n) => `- ${n.text}`).join("\n")}\n`).join("\n") +
    `\nMore, generated: .local/docs/{fleet,lessons,measurements,upstreams,chromium-layout}.md\n`;
  await Bun.write(join(root, ".claude.local.md"), text);
  return { dir, notes: notes.length, lessons: lessons.length, hosts: hosts.length };
}
