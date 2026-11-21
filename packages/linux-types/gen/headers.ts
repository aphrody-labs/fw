#!/usr/bin/env bun
// Produces the sanitized UAPI headers the generator reads: a sparse partial clone of the stable
// tree pinned in kernel.toml (only scripts/, include/uapi, arch headers and syscall tables: about
// 40 MB) and `make headers_install` per arch into <out>/<arch>/. No kernel is built.
//
//   bun gen/headers.ts --out /srv/xbuild/scratch/linux-types/headers [--tree stable] [--arch x86_64,aarch64]
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { ARCHES, archOf } from "./lib/arch.ts";
import { Timings, run, runOk } from "./lib/exec.ts";

const SPARSE = [
  "/*",
  "!/*/",
  "/scripts/",
  "/include/uapi/",
  "/include/generated/",
  "/arch/*/include/uapi/",
  "/arch/*/include/generated/",
  "/arch/*/Makefile*",
  "/arch/*/Kbuild*",
  "/arch/*/entry/syscalls/",
  "/arch/*/kernel/syscalls/",
  "/tools/include/uapi/",
];

const { values } = parseArgs({ options: { out: { type: "string" }, tree: { type: "string", default: "stable" }, arch: { type: "string" } }, strict: true });
if (!values.out) {
  console.error("usage: bun gen/headers.ts --out <dir> [--tree stable] [--arch x86_64,aarch64]");
  process.exit(2);
}
const out = resolve(values.out);
const arches = (values.arch ? values.arch.split(",") : Object.keys(ARCHES)).map(archOf);

const pins = Bun.TOML.parse(await readFile(resolve(import.meta.dir, "../../../kernel.toml"), "utf8")) as { tree: { id: string; repo: string; tag: string; commit: string }[] };
const tree = pins.tree.find((entry) => entry.id === values.tree);
if (!tree) throw new Error(`kernel.toml has no tree ${values.tree}`);

const timings = new Timings();
const source = join(out, ".source");
await mkdir(source, { recursive: true });
const git = (...args: string[]) => runOk(["git", "-C", source, ...args]);

await timings.time("sparse clone", async () => {
  const head = await run(["git", "-C", source, "rev-parse", "--verify", "-q", "HEAD"]);
  if (head.code === 0 && head.stdout.trim() === tree.commit) return;
  await runOk(["git", "init", "-q", source]);
  const remote = await run(["git", "-C", source, "remote", "get-url", "origin"]);
  if (remote.code !== 0) await git("remote", "add", "origin", `https://github.com/${tree.repo}.git`);
  await git("config", "extensions.partialClone", "origin");
  await git("config", "core.sparseCheckout", "true");
  await writeFile(join(source, ".git/info/sparse-checkout"), `${SPARSE.join("\n")}\n`);
  await git("fetch", "-q", "--depth", "1", "--filter=blob:none", "origin", tree.commit);
  await git("checkout", "-q", "FETCH_HEAD");
});

for (const arch of arches) {
  await timings.time(`headers_install ${arch.name}`, async () => {
    await runOk(["make", "-s", `ARCH=${arch.kernelArch}`, `INSTALL_HDR_PATH=${join(out, arch.name)}`, "headers_install"], { cwd: source });
  });
}
console.error(`${tree.tag} ${tree.commit} -> ${arches.map((arch) => join(out, arch.name)).join(", ")}`);
for (const { stage, seconds } of timings.stages) console.error(`  ${stage}: ${seconds.toFixed(1)} s`);
