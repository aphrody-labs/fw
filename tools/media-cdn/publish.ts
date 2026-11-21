#!/usr/bin/env bun
// media-cdn step 2: upload the built /v1 tree to the R2 bucket behind cloudflare/media-cdn (S3 API through ../material-cdn/r2.ts).
//   CLOUDFLARE_API_TOKEN=... bun tools/media-cdn/publish.ts [--out DIR] [--bucket NAME] [--dry] [--concurrency N]
// Only objects that differ from the bucket (ETag = MD5, multipart ETags are compared by size) are written. Every key contains the content
// hash, so nothing is ever overwritten with different bytes and nothing is deleted: older pinned URLs keep working. The manifest goes last,
// merged over the bucket's own: names this run did not build stay as they are.
import { readdir, stat } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";
import type { Manifest } from "./manifest.ts";
import { openBucket, put } from "../material-cdn/r2.ts";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1]! : fallback;
};
const out = resolve(opt("out", ".cache/media-cdn-out"));
const bucketName = opt("bucket", "aphrody-media");
const dry = args.includes("--dry");
const concurrency = Number(opt("concurrency", "8"));
const bucket = await openBucket(bucketName);
const { s3 } = bucket;

const TYPES: Record<string, string> = {
  avif: "image/avif",
  webp: "image/webp",
  jpg: "image/jpeg",
  png: "image/png",
  svg: "image/svg+xml",
  mp4: "video/mp4",
  webm: "video/webm",
  json: "application/json; charset=utf-8",
};
const typeOf = (key: string) => TYPES[key.split(".").pop()!] ?? "application/octet-stream";

const files: string[] = [];
async function walk(dir: string) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) await walk(path);
    else if (!entry.name.startsWith(".")) files.push(path);
  }
}
await walk(join(out, "v1"));
const keyOf = (path: string) => relative(out, path).split(sep).join("/");

const existing = new Map<string, { etag: string; size: number }>();
let startAfter: string | undefined;
for (;;) {
  const page = await s3.list({ prefix: "v1/", maxKeys: 1000, startAfter });
  for (const o of page.contents ?? []) existing.set(o.key, { etag: (o.eTag ?? "").replaceAll('"', ""), size: o.size ?? -1 });
  if (!page.isTruncated || !page.contents?.length) break;
  startAfter = page.contents[page.contents.length - 1]!.key;
}
console.log(`bucket ${bucketName}: ${existing.size} existing objects, ${files.length} local files`);

async function md5(path: string): Promise<string> {
  const hasher = new Bun.CryptoHasher("md5");
  for await (const chunk of Bun.file(path).stream()) hasher.update(chunk);
  return hasher.digest("hex");
}

const manifestPath = join(out, "v1/manifest.json");
const todo: { path: string; key: string; size: number }[] = [];
let same = 0;
for (const path of files) {
  const key = keyOf(path);
  if (path === manifestPath) continue;
  const size = (await stat(path)).size;
  const remote = existing.get(key);
  // Multipart ETags (`<md5>-<parts>`) cannot be recomputed from the file; the key holds the content hash, so the size decides.
  if (remote && (remote.etag.includes("-") ? remote.size === size : remote.etag === (await md5(path)))) same++;
  else todo.push({ path, key, size });
}
const bytes = todo.reduce((n, t) => n + t.size, 0);
console.log(`${same} unchanged, ${todo.length} to upload (${bytes} bytes)${dry ? " [dry run]" : ""}`);

if (!dry) {
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(concurrency, todo.length) }, async () => {
      while (next < todo.length) {
        const { path, key } = todo[next++]!;
        await put(bucket, key, new Uint8Array(await Bun.file(path).arrayBuffer()), typeOf(key));
      }
    }),
  );

  const local = JSON.parse(await Bun.file(manifestPath).text()) as Manifest;
  const remoteObject = s3.file("v1/manifest.json");
  const remote: Manifest = (await remoteObject.exists()) ? JSON.parse(await remoteObject.text()) : { schema: 1, revision: "", images: {}, video: {}, sprites: {} };
  const merged: Manifest = { schema: 1, revision: "", images: { ...remote.images, ...local.images }, video: { ...remote.video, ...local.video }, sprites: { ...remote.sprites, ...local.sprites } };
  merged.revision = new Bun.CryptoHasher("sha256").update(JSON.stringify({ ...merged, revision: "" })).digest("hex").slice(0, 8);
  await put(bucket, "v1/manifest.json", JSON.stringify(merged, null, 1) + "\n", TYPES.json!);
  console.log(`manifest ${merged.revision}: ${Object.keys(merged.images).length} images, ${Object.keys(merged.video).length} videos, ${Object.keys(merged.sprites).length} sprites`);
}
