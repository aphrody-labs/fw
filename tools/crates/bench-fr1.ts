#!/usr/bin/env bun
// Runs a `fw crates bench` suite on the build host (fr1 by default), never on this machine, through tools/fr1.ts:
// git is the transport, the host's single checkout builds fw-crates in the shared target, the suite runs with the
// servers and the load generator on disjoint CPU halves, and the JSON and MDX results are copied back.
//
//   bun tools/crates/bench-fr1.ts [--suite web] [--out docs/crates/bench] [--work /srv/xbuild/crates-bench] [-- <extra fw-crates bench flags>]
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const argv = Bun.argv.slice(2);
const split = argv.indexOf("--");
const own = split === -1 ? argv : argv.slice(0, split);
const extra = split === -1 ? [] : argv.slice(split + 1);
const opt = (name: string, fallback: string): string => {
  const i = own.indexOf(`--${name}`);
  return i >= 0 && own[i + 1] ? own[i + 1] : fallback;
};

const host = process.env.FW_BUILD_HOST ?? "fr1";
const suite = opt("suite", "web");
const work = opt("work", "/srv/xbuild/crates-bench");
const out = resolve(opt("out", "docs/crates/bench"));
const fr1 = resolve(import.meta.dir, "..", "fr1.ts");

const remote = [
  "set -e",
  "cargo build --release -p fw-crates",
  'bin="${CARGO_TARGET_DIR:-target}/release/fw-crates"',
  "n=$(nproc); h=$((n/2))",
  `mkdir -p ${work}; rm -rf ${work}/out`,
  `"$bin" bench ${suite} --label ${host} --server-cpus 0-$((h-1)) --load-cpus $h-$((n-1)) --workdir ${work}/work --out ${work}/out ${extra.join(" ")} > ${work}/last-result.txt`,
].join("; ");

const run = Bun.spawnSync(["bun", fr1, "run", "--", "bash", "-c", remote], { stdout: "inherit", stderr: "inherit" });
if (run.exitCode !== 0) process.exit(run.exitCode ?? 1);

mkdirSync(join(out, suite), { recursive: true });
const tar = Bun.spawnSync(["ssh", host, `tar cf - -C ${work}/out/${suite} .`], { stdout: "pipe", stderr: "inherit" });
if (tar.exitCode !== 0) process.exit(tar.exitCode ?? 1);
const untar = Bun.spawnSync(["tar", "xf", "-", "-C", join(out, suite)], { stdin: tar.stdout, stderr: "inherit" });
console.log(JSON.stringify({ ok: untar.exitCode === 0, host, suite, out: join(out, suite) }));
process.exit(untar.exitCode ?? 1);
