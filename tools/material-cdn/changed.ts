#!/usr/bin/env bun
// material-cdn: compare the freshly synced sources with the live index and report whether a rebuild is needed.
//   bun tools/material-cdn/changed.ts [--work DIR] [--base URL]   prints changed=true|false (also to $GITHUB_OUTPUT)
// Only the pinned inputs are compared (icons commit, font release tags), not the repository names, so reading the
// upstream directly or through a fork gives the same answer.
import { appendFileSync } from "node:fs";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const work = resolve(opt("work", ".cache/material-cdn"));
const base = opt("base", process.env.MATERIAL_CDN_BASE ?? "");
const local = await Bun.file(join(work, "sources.json")).json();
const key = (s: any) => JSON.stringify([s.icons?.sha, s.flex?.tag, s.flex?.zipSha256, s.code?.tag, s.code?.zipSha256]);
let changed = true;
if (base) {
  const r = await fetch(`${base.replace(/\/$/, "")}/index.json`, { headers: { "Cache-Control": "no-cache" } });
  if (r.ok) changed = key((await r.json() as any).sources) !== key(local);
}
console.log(`changed=${changed}`);
if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `changed=${changed}\n`);
