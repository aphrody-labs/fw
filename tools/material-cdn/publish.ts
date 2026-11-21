#!/usr/bin/env bun
// material-cdn step 3: upload the built /v1 tree to the R2 bucket behind cloudflare/material-cdn (S3 API, no REST rate limit).
//   CLOUDFLARE_API_TOKEN=... bun tools/material-cdn/publish.ts [--out DIR] [--bucket NAME] [--dry] [--prune] [--force] [--concurrency N]
// Only changed objects are written (ETag = MD5 compared with the bucket listing); index.json goes last so a release is
// visible only once everything it references exists. Content-addressed fonts and versioned tarballs are never deleted.
// R2 S3 credentials are derived from the API token as documented by Cloudflare: access key id = token id, secret = SHA-256(token).
import { readdir } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const out = resolve(opt("out", ".cache/material-cdn-out"));
const bucket = opt("bucket", "aphrody-material");
const dry = args.includes("--dry");
const prune = args.includes("--prune");
const token = process.env.CLOUDFLARE_API_TOKEN;
if (!token) throw new Error("CLOUDFLARE_API_TOKEN is not set");

const cf = async <T>(path: string): Promise<T> => {
  const r = await fetch(`https://api.cloudflare.com/client/v4/${path}`, { headers: { Authorization: `Bearer ${token}` } });
  const j = (await r.json()) as { success: boolean; result: T; errors: unknown[] };
  if (!j.success) throw new Error(`Cloudflare ${path}: ${JSON.stringify(j.errors)}`);
  return j.result;
};
const accountId = process.env.CLOUDFLARE_ACCOUNT_ID ?? (await cf<{ id: string }[]>("accounts"))[0]!.id;
const tokenId = (await cf<{ id: string }>(`accounts/${accountId}/tokens/verify`)).id;
const s3 = new Bun.S3Client({
  accessKeyId: tokenId,
  secretAccessKey: new Bun.CryptoHasher("sha256").update(token).digest("hex"),
  endpoint: `https://${accountId}.r2.cloudflarestorage.com`,
  bucket,
  region: "auto",
});

const TYPES: Record<string, string> = {
  svg: "image/svg+xml",
  json: "application/json; charset=utf-8",
  css: "text/css; charset=utf-8",
  woff2: "font/woff2",
  rs: "text/plain; charset=utf-8",
  gz: "application/gzip",
  txt: "text/plain; charset=utf-8",
  sha256: "text/plain; charset=utf-8",
};

const files: string[] = [];
async function walk(d: string) {
  for (const e of await readdir(d, { withFileTypes: true })) {
    const p = join(d, e.name);
    if (e.isDirectory()) await walk(p);
    else files.push(p);
  }
}
await walk(join(out, "v1"));
const keyOf = (p: string) => relative(out, p).split(sep).join("/");

// Existing objects: key -> etag (MD5).
const existing = new Map<string, string>();
let startAfter: string | undefined;
for (;;) {
  const page = await s3.list({ prefix: "v1/", maxKeys: 1000, startAfter });
  for (const o of page.contents ?? []) existing.set(o.key, (o.eTag ?? "").replaceAll('"', ""));
  if (!page.isTruncated || !page.contents?.length) break;
  startAfter = page.contents[page.contents.length - 1]!.key;
}
console.log(`bucket ${bucket}: ${existing.size} existing objects, ${files.length} local files`);

const md5 = (b: Uint8Array) => new Bun.CryptoHasher("md5").update(b).digest("hex");
const index = join(out, "v1", "index.json");
const todo: { p: string; key: string; body: Uint8Array }[] = [];
let same = 0;
let i = 0;
await Promise.all(
  Array.from({ length: 32 }, async () => {
    while (i < files.length) {
      const p = files[i++]!;
      const body = new Uint8Array(await Bun.file(p).arrayBuffer());
      const key = keyOf(p);
      if (existing.get(key) === md5(body)) same++;
      else todo.push({ p, key, body });
    }
  }),
);
// Content-addressed fonts and versioned tarballs are served as immutable: an existing key must never change bytes.
const IMMUTABLE_KEY = /(\.[0-9a-f]{8}\.woff2|^v1\/rust\/\d+\.\d+\.\d+\.tar\.gz(\.sha256)?)$/;
const clobbered = todo.filter((t) => IMMUTABLE_KEY.test(t.key) && existing.has(t.key));
if (clobbered.length && !args.includes("--force")) {
  throw new Error(`refusing to overwrite immutable objects (content changed under the same name): ${clobbered.map((t) => t.key).join(", ")}; pass --force to replace them`);
}
todo.sort((a, b) => (a.p === index ? 1 : b.p === index ? -1 : 0)); // index.json last
console.log(`unchanged ${same}, to upload ${todo.length} (${todo.reduce((n, t) => n + t.body.length, 0)} bytes)${dry ? " [dry run]" : ""}`);

// R2 answers InternalError / 429 under bursts: retry with exponential backoff and jitter.
async function put(key: string, body: Uint8Array, type: string) {
  for (let attempt = 0; ; attempt++) {
    try {
      await s3.write(key, body, { type });
      return;
    } catch (e) {
      if (attempt >= 6) throw e;
      await Bun.sleep(250 * 2 ** attempt + Math.random() * 250);
    }
  }
}

if (!dry) {
  const rest = todo.filter((t) => t.p !== index);
  let done = 0;
  let next = 0;
  await Promise.all(
    Array.from({ length: Number(opt("concurrency", "128")) }, async () => {
      while (next < rest.length) {
        const t = rest[next++]!;
        await put(t.key, t.body, TYPES[t.key.split(".").pop()!] ?? "application/octet-stream");
        if (++done % 10000 === 0) console.log(`  ${done}/${rest.length}`);
      }
    }),
  );
  const last = todo.find((t) => t.p === index);
  if (last) await put(last.key, last.body, TYPES.json!);

  if (prune) {
    const keep = new Set(files.map(keyOf));
    const stale = [...existing.keys()].filter((k) => !keep.has(k) && /^v1\/(icons|sprites|manifest)\//.test(k));
    for (const k of stale) await s3.delete(k);
    console.log(`pruned ${stale.length} stale objects`);
  }

  // Edge cache for the mutable aliases: purge the whole hostname when a custom domain is configured.
  const host = process.env.MATERIAL_CDN_HOST;
  if (host && todo.length) {
    const zones = await cf<{ id: string; name: string }[]>(`zones?name=${host.split(".").slice(-2).join(".")}`);
    const r = await fetch(`https://api.cloudflare.com/client/v4/zones/${zones[0]!.id}/purge_cache`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({ hosts: [host] }),
    });
    console.log(`purge ${host}: ${r.status}`);
  }
}
console.log(`published ${todo.length} objects`);
