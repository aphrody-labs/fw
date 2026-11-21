#!/usr/bin/env bun
// material-cdn: publish files of another worker (sprite sheets, GLB models, textures, ...) under a versioned, immutable path.
//   CLOUDFLARE_API_TOKEN=... bun tools/material-cdn/asset.ts <name> <version> <file|dir>... [--base DIR] [--bucket NAME] [--dry]
// Served as https://material.aphrody.com/v1/assets/<name>/<version>/<path> with `Cache-Control: immutable`, so a version is never rewritten:
// an existing object with other bytes aborts the run. A manifest.json (path, bytes, sha256 of every file) is written next to the files.
// <name> is [a-z0-9_-]+, <version> a semver or any label with a digit (`1.2.0`, `2026-11-21`, `r4`). Paths keep their position under --base
// (default: the directory of each file argument, or the directory itself).
import { readdir, stat } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import { openBucket, put, typeOf } from "./r2.ts";

const args = process.argv.slice(2);
const flag = (name: string) => {
  const i = args.indexOf(`--${name}`);
  if (i < 0) return undefined;
  return args.splice(i, 2)[1];
};
const dry = args.includes("--dry") && !!args.splice(args.indexOf("--dry"), 1);
const bucketName = flag("bucket") ?? "aphrody-material";
const baseArg = flag("base");
const [name, version, ...inputs] = args;
if (!name || !version || !inputs.length) throw new Error("usage: asset.ts <name> <version> <file|dir>... [--base DIR] [--bucket NAME] [--dry]");
if (!/^[a-z0-9_-]+$/.test(name)) throw new Error(`name ${name}: use [a-z0-9_-]`);
if (!/^[A-Za-z0-9._+-]*\d[A-Za-z0-9._+-]*$/.test(version)) throw new Error(`version ${version}: use letters, digits and . _ + - with at least one digit`);

const files: { path: string; rel: string }[] = [];
async function add(path: string, base: string) {
  if ((await stat(path)).isDirectory()) {
    for (const e of await readdir(path, { withFileTypes: true })) await add(join(path, e.name), base);
  } else files.push({ path, rel: relative(base, path).split(sep).join("/") });
}
for (const input of inputs) {
  const path = resolve(input);
  const isDir = (await stat(path)).isDirectory();
  await add(path, baseArg ? resolve(baseArg) : isDir ? path : dirname(path));
}
const seen = new Set<string>();
for (const f of files) {
  if (!/^[A-Za-z0-9_\-./]+$/.test(f.rel) || f.rel.includes("..")) throw new Error(`path ${f.rel}: use [A-Za-z0-9_-./]`);
  if (f.rel === "manifest.json" || seen.has(f.rel)) throw new Error(`duplicate or reserved path ${f.rel}`);
  seen.add(f.rel);
}

const sha256 = (b: Uint8Array) => new Bun.CryptoHasher("sha256").update(b).digest("hex");
const prefix = `v1/assets/${name}/${version}/`;
const bodies = await Promise.all(files.map(async (f) => ({ ...f, body: new Uint8Array(await Bun.file(f.path).arrayBuffer()) })));
const manifest = JSON.stringify({ name, version, files: bodies.map((f) => ({ path: f.rel, bytes: f.body.length, sha256: sha256(f.body) })) });
const all = [...bodies.map((f) => ({ key: prefix + f.rel, body: f.body })), { key: `${prefix}manifest.json`, body: new TextEncoder().encode(manifest) }];

const client = await openBucket(bucketName);
const md5 = (b: Uint8Array) => new Bun.CryptoHasher("md5").update(b).digest("hex");
const existing = new Map<string, string>();
for (let startAfter: string | undefined; ; ) {
  const page = await client.s3.list({ prefix, maxKeys: 1000, startAfter });
  for (const o of page.contents ?? []) existing.set(o.key, (o.eTag ?? "").replaceAll('"', ""));
  if (!page.isTruncated || !page.contents?.length) break;
  startAfter = page.contents[page.contents.length - 1]!.key;
}
const clobbered = all.filter((o) => existing.has(o.key) && existing.get(o.key) !== md5(o.body));
if (clobbered.length) throw new Error(`${prefix} is immutable: these objects exist with other bytes: ${clobbered.map((o) => o.key).join(", ")}; publish a new version`);
const todo = all.filter((o) => !existing.has(o.key));
for (const o of todo) {
  console.log(`${dry ? "would put" : "put"} ${o.key} ${o.body.length} bytes`);
  if (!dry) await put(client, o.key, o.body, typeOf(o.key));
}
console.log(`https://material.aphrody.com/${prefix}manifest.json: ${all.length} files, ${todo.length} uploaded`);
