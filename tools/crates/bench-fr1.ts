#!/usr/bin/env bun
// Runs a `fw crates bench` suite on the build host (fr1 by default), never on this machine: clones or fast-forwards a
// scratch checkout of origin/main there, builds fw-crates in release, runs the suite pinned to disjoint CPU sets, and
// copies the JSON and MDX results back into docs/crates/bench.
//
//   bun tools/crates/bench-fr1.ts [--host fr1] [--suite web] [--out docs/crates/bench] [--dir /srv/xbuild/crates-bench] [-- <extra fw-crates bench flags>]
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

const host = opt("host", "fr1");
const suite = opt("suite", "web");
const dir = opt("dir", "/srv/xbuild/crates-bench");
const out = resolve(opt("out", "docs/crates/bench"));
const remoteOut = `${dir}/out`;

function ssh(script: string): void {
  const p = Bun.spawnSync(["ssh", "-o", "ServerAliveInterval=30", host, script], { stdout: "inherit", stderr: "inherit" });
  if (p.exitCode !== 0) {
    console.error(`bench-fr1: remote step failed (exit ${p.exitCode}): ${script.slice(0, 200)}`);
    process.exit(p.exitCode ?? 1);
  }
}

const repo = "https://github.com/aphrody-labs/fw.git";
ssh(`set -e; mkdir -p ${dir}; cd ${dir}; if [ -d fw/.git ]; then git -C fw fetch -q --depth 1 origin main && git -C fw reset -q --hard origin/main; else git clone -q --depth 1 ${repo} fw; fi`);
ssh(`set -e; cd ${dir}/fw; export CARGO_TARGET_DIR=${dir}/target; cargo build --release -p fw-crates`);
// Half the CPUs for the servers, the other half for the load generator, so the two never share a core.
ssh(`set -e; n=$(nproc); h=$((n/2)); cd ${dir}/fw; rm -rf ${remoteOut}; ${dir}/target/release/fw-crates bench ${suite} --label ${host} --server-cpus 0-$((h-1)) --load-cpus $h-$((n-1)) --workdir ${dir}/work --out ${remoteOut} ${extra.join(" ")} > ${dir}/last-result.txt`);

mkdirSync(join(out, suite), { recursive: true });
const tar = Bun.spawnSync(["ssh", host, `tar cf - -C ${remoteOut}/${suite} .`], { stdout: "pipe", stderr: "inherit" });
if (tar.exitCode !== 0) process.exit(tar.exitCode ?? 1);
const untar = Bun.spawnSync(["tar", "xf", "-", "-C", join(out, suite)], { stdin: tar.stdout, stderr: "inherit" });
console.log(JSON.stringify({ ok: untar.exitCode === 0, host, suite, out: join(out, suite) }));
process.exit(untar.exitCode ?? 1);
