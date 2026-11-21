#!/usr/bin/env bun
// material-cdn step 4: pin crates/fw-material to the tarball just built (version, SHA-256, base URL) and refresh its vendored tables.
//   bun tools/material-cdn/pin.ts [--out DIR] [--base URL]
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const out = resolve(opt("out", ".cache/material-cdn-out"));
const crate = resolve(import.meta.dir, "..", "..", "crates", "fw-material");
const index = JSON.parse(await readFile(join(out, "v1", "index.json"), "utf8"));
const { crateVersion: version, crateSha256: sha256 } = index.rust as { crateVersion: string; crateSha256: string };
const base = opt("base", process.env.MATERIAL_CDN_BASE ?? (await readFile(join(crate, "vendor", "PIN"), "utf8").then((t) => /base = "([^"]+)"/.exec(t)?.[1]).catch(() => undefined)) ?? "");
if (!base) throw new Error("no CDN base: pass --base or set MATERIAL_CDN_BASE");

const tgz = join(out, "v1", "rust", `${version}.tar.gz`);
const tmp = await mkdtemp(join(tmpdir(), "material-pin-"));
const p = Bun.spawn(["tar", "-xzf", tgz, "-C", tmp, "--strip-components=1"], { stderr: "inherit" });
if ((await p.exited) !== 0) throw new Error(`tar -xzf ${tgz} failed`);
await mkdir(join(crate, "vendor"), { recursive: true });
for (const f of ["codepoints.rs", "outlined.rs", "outlined_fill.rs"]) await copyFile(join(tmp, "src", f), join(crate, "vendor", f));
await copyFile(join(tmp, "LICENSE"), join(crate, "vendor", "LICENSE"));
await copyFile(join(tmp, "NOTICE"), join(crate, "vendor", "NOTICE"));
await writeFile(
  join(crate, "vendor", "PIN"),
  `# Written by tools/material-cdn/pin.ts. build.rs verifies the CDN tarball against sha256 before using it.\nversion = "${version}"\nsha256 = "${sha256}"\nbase = "${base}"\n`,
);
await rm(tmp, { recursive: true, force: true });
console.log(`pinned fw-material to ${version} ${sha256}`);
