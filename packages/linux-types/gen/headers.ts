#!/usr/bin/env bun
// Produces the sanitized UAPI headers the generator reads: `make headers_install` per arch from the
// stable kernel tree pinned in kernel.toml, into <out>/<arch>/. No kernel is built.
//
// The tree is the sparse checkout `fw kernel sync` makes (profile "uapi" of kernel.toml) at
// <fw root>/<tree path>. When it is not at the pinned commit, fw-linux is asked to sync it
// ($FW_LINUX_BIN, else `fw-linux` on PATH, else `fw kernel`). The build directory (O=) is outside
// the tree, so the tree stays clean.
//
//   bun gen/headers.ts --out <dir> [--tree stable] [--arch x86_64,aarch64] [--tree-dir <synced tree>]
import { mkdir, readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { ARCHES, archOf } from "./lib/arch.ts";
import { Timings, run, runOk } from "./lib/exec.ts";

const { values } = parseArgs({
  options: { out: { type: "string" }, tree: { type: "string", default: "stable" }, arch: { type: "string" }, "tree-dir": { type: "string" } },
  strict: true,
});
if (!values.out) {
  console.error("usage: bun gen/headers.ts --out <dir> [--tree stable] [--arch x86_64,aarch64] [--tree-dir <synced tree>]");
  process.exit(2);
}
const out = resolve(values.out);
const arches = (values.arch ? values.arch.split(",") : Object.keys(ARCHES)).map(archOf);
const fwRoot = resolve(import.meta.dir, "../../..");

const pins = Bun.TOML.parse(await readFile(join(fwRoot, "kernel.toml"), "utf8")) as { tree: { id: string; path: string; tag: string; commit: string; profile?: string }[] };
const tree = pins.tree.find((entry) => entry.id === values.tree);
if (!tree) throw new Error(`kernel.toml has no tree ${values.tree}`);
const treeDir = resolve(values["tree-dir"] ?? join(fwRoot, tree.path));

async function headCommit(dir: string): Promise<string> {
  const result = await run(["git", "-C", dir, "rev-parse", "HEAD"]);
  return result.code === 0 ? result.stdout.trim() : "";
}

async function syncCommand(): Promise<string[]> {
  const candidates: string[][] = [];
  if (process.env.FW_LINUX_BIN) candidates.push([process.env.FW_LINUX_BIN]);
  candidates.push(["fw-linux"], ["fw", "kernel"]);
  for (const candidate of candidates) {
    const path = candidate[0] as string;
    if (path.includes("/") ? await Bun.file(path).exists() : Bun.which(path)) return candidate;
  }
  throw new Error(`${treeDir} is not at ${tree?.commit} and no sync tool was found: set FW_LINUX_BIN, put fw-linux on PATH, or install fw (fw kernel sync ${tree?.id} --profile ${tree?.profile ?? "uapi"})`);
}

const timings = new Timings();
if ((await headCommit(treeDir)) !== tree.commit) {
  if (values["tree-dir"]) throw new Error(`--tree-dir ${treeDir} is at ${(await headCommit(treeDir)) || "no commit"}, kernel.toml pins ${tree.commit}`);
  await timings.time("fw kernel sync", async () => {
    const command = await syncCommand();
    await runOk([...command, "sync", tree.id, "--profile", tree.profile ?? "uapi", "--root", fwRoot, "--json"]);
  });
  const now = await headCommit(treeDir);
  if (now !== tree.commit) throw new Error(`after sync ${treeDir} is at ${now || "no commit"}, kernel.toml pins ${tree.commit}`);
}

for (const arch of arches) {
  await timings.time(`headers_install ${arch.name}`, async () => {
    const build = join(out, ".build", arch.name);
    await mkdir(build, { recursive: true });
    await runOk(["make", "-s", `O=${build}`, `ARCH=${arch.kernelArch}`, `INSTALL_HDR_PATH=${join(out, arch.name)}`, "headers_install"], { cwd: treeDir });
  });
}
console.error(`${tree.tag} ${tree.commit} from ${treeDir} -> ${arches.map((arch) => join(out, arch.name)).join(", ")}`);
for (const { stage, seconds } of timings.stages) console.error(`  ${stage}: ${seconds.toFixed(1)} s`);
