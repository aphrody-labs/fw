#!/usr/bin/env bun
// Wall time of `ssh <host> true` per node of cluster/build.toml, with the remote load average read in the same session.
//   bun tools/modern-toolchain/bench/ssh-latency.ts [--runs 7] [--config <ssh config file>] [--hosts a,b]
import { join } from "node:path";

const flag = (name: string) => {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 ? process.argv[index + 1] : undefined;
};
const runs = Number(flag("runs") || 7);
const config = flag("config") ? ["-F", flag("config")!] : [];
const build = Bun.TOML.parse(await Bun.file(join(import.meta.dir, "..", "..", "..", "cluster", "build.toml")).text()) as {
  hosts?: Record<string, { ssh?: string }>;
};
const hosts = flag("hosts")?.split(",") ?? Object.entries(build.hosts ?? {}).map(([name, host]) => host.ssh ?? name);
let failed = false;

const median = (values: number[]) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const timed = async (argv: string[]) => {
  const start = performance.now();
  const run = Bun.spawn(argv, { stdout: "pipe", stderr: "ignore", stdin: "ignore" });
  const out = await new Response(run.stdout).text();
  return { code: await run.exited, ms: performance.now() - start, out };
};

for (const host of hosts) {
  const samples: number[] = [];
  for (let index = 0; index < runs; index++) {
    const result = await timed(["ssh", ...config, "-o", "BatchMode=yes", host, "true"]);
    if (result.code === 0) samples.push(result.ms);
  }
  if (samples.length === 0) failed = true;
  const load = (await timed(["ssh", ...config, "-o", "BatchMode=yes", host, "cut -d' ' -f1-3 /proc/loadavg; nproc"])).out.trim().replace("\n", " cores ");
  console.log(JSON.stringify({ host, ok: samples.length, min: Math.round(Math.min(...samples)), median: Math.round(median(samples)), max: Math.round(Math.max(...samples)), loadavg_cores: load }));
}
if (failed) process.exit(1);
