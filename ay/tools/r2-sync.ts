#!/usr/bin/env bun
// The one R2 sync tool. Buckets are named by apps/edge/edge.toml [buckets] (a binding such as ASSETS, never a bucket name typed here),
// credentials come from CLOUDFLARE_ACCOUNT_ID, R2_ACCESS_KEY_ID and R2_SECRET_ACCESS_KEY. Layout of the assets bucket (what the edge Worker serves):
//   brand/<file>                     identity files of brand/assets (icon.svg, favicon.ico, apple-touch-icon.png, manifest): the design pipeline's output
//   v1/<kind>/<sha256>.<ext>         content-addressed objects, immutable (ay-cf-gateway kinds: icon ui hdri lottie tex glb; loop stream segment go to VIDEO)
//   <prefix>/<upstream path>         mirrors of an upstream CDN
// Releases are not synced here: tools/release-publish.ts owns that layout.
//   bun tools/r2-sync.ts brand [--bucket ASSETS] [--dir brand/assets] [--dry-run]
//   bun tools/r2-sync.ts assets --kind <kind> [--bucket ASSETS|VIDEO] [--dry-run] <file...>
//   bun tools/r2-sync.ts mirror --prefix <p> [--bucket ASSETS] [--dry-run] <url...>
import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { S3Client } from "bun";

const root = resolve(import.meta.dir, "..");

export interface R2SyncConfig {
  bucket: string;
  accountId: string;
  accessKeyId: string;
  secretAccessKey: string;
}

export interface SyncResult {
  source: string;
  key: string;
  bytes: number;
  contentType: string;
  sha256: string;
  uploaded: boolean;
  /** The object was already there with the same bytes. */
  unchanged?: boolean;
}

/** The slice of Bun's S3 file the tool needs, so tests can stand in for R2. */
export interface Store {
  file(key: string): { exists(): Promise<boolean>; write(data: Uint8Array, opts?: { type?: string }): Promise<unknown>; stat?(): Promise<{ etag?: string; size?: number }> };
}

const TYPES: Record<string, string> = {
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".webp": "image/webp",
  ".webmanifest": "application/manifest+json",
  ".json": "application/json",
  ".woff2": "font/woff2",
  ".wasm": "application/wasm",
  ".glb": "model/gltf-binary",
  ".mp4": "video/mp4",
  ".m3u8": "application/vnd.apple.mpegurl",
  ".ts": "video/mp2t",
};
export const contentTypeOf = (name: string): string => TYPES[extname(name).toLowerCase()] ?? "application/octet-stream";

export const KINDS = ["icon", "ui", "hdri", "lottie", "tex", "glb", "loop", "stream", "segment"] as const;
export const VIDEO_KINDS = new Set(["loop", "stream", "segment"]);

/** Bucket name behind a binding of apps/edge/edge.toml [buckets]. */
export function bucketOf(binding: string, path = join(root, "apps/edge/edge.toml")): string {
  const buckets = (Bun.TOML.parse(readFileSync(path, "utf8")) as { buckets?: Record<string, string> }).buckets ?? {};
  const name = buckets[binding];
  if (!name) throw new Error(`${path}: no bucket bound as ${binding} (have ${Object.keys(buckets).join(", ")})`);
  return name;
}

export function createR2Client(cfg: R2SyncConfig): S3Client {
  return new S3Client({
    accessKeyId: cfg.accessKeyId,
    secretAccessKey: cfg.secretAccessKey,
    bucket: cfg.bucket,
    endpoint: `https://${cfg.accountId}.r2.cloudflarestorage.com`,
  });
}

export function clientFor(binding: string): S3Client {
  const need = (k: string) => process.env[k] || (() => { throw new Error(`${k} is not set`); })();
  return createR2Client({ bucket: bucketOf(binding), accountId: need("CLOUDFLARE_ACCOUNT_ID"), accessKeyId: need("R2_ACCESS_KEY_ID"), secretAccessKey: need("R2_SECRET_ACCESS_KEY") });
}

export const sha256Hex = (bytes: Uint8Array): string => new Bun.CryptoHasher("sha256").update(bytes).digest("hex");

export function objectKeyForUrl(sourceUrl: string, prefix = ""): string {
  const u = new URL(sourceUrl);
  const cleanPrefix = prefix.replace(/^\/+|\/+$/g, "");
  const cleanPath = u.pathname.replace(/^\/+/, "");
  return cleanPrefix ? `${cleanPrefix}/${cleanPath}` : cleanPath;
}

/** Content-addressed key of the gateway layout: v1/<kind>/<sha256>.<ext>. */
export function addressedKey(kind: string, bytes: Uint8Array, name: string): string {
  if (!(KINDS as readonly string[]).includes(kind)) throw new Error(`kind ${kind} is not one of ${KINDS.join(", ")}`);
  const ext = extname(name).slice(1).toLowerCase();
  if (!/^[a-z0-9]+$/.test(ext)) throw new Error(`${name}: the extension is required for a content-addressed key`);
  return `v1/${kind}/${sha256Hex(bytes)}.${ext}`;
}

async function put(store: Store | undefined, source: string, key: string, bytes: Uint8Array, contentType: string, opts: { dryRun?: boolean; skipExisting?: boolean }): Promise<SyncResult> {
  const sha256 = sha256Hex(bytes);
  const base = { source, key, bytes: bytes.byteLength, contentType, sha256 };
  if (opts.dryRun) return { ...base, uploaded: false };
  if (!store) throw new Error("a store is required when dryRun is false");
  const object = store.file(key);
  if (opts.skipExisting && (await object.exists())) return { ...base, uploaded: false, unchanged: true };
  await object.write(bytes, { type: contentType });
  return { ...base, uploaded: true };
}

export async function mirrorUrlToR2(
  sourceUrl: string,
  opts: { client?: Store; prefix?: string; dryRun?: boolean; fetcher?: (input: string) => Promise<Response> },
): Promise<SyncResult> {
  const res = await (opts.fetcher ?? fetch)(sourceUrl);
  if (!res.ok) throw new Error(`upstream ${res.status} for ${sourceUrl}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  return put(opts.client, sourceUrl, objectKeyForUrl(sourceUrl, opts.prefix), bytes, res.headers.get("content-type") ?? "application/octet-stream", { dryRun: opts.dryRun });
}

/** Files directly in `dir` (the identity files of the design pipeline) to brand/<name>; unchanged objects are skipped by name and size. */
export async function syncBrand(dir: string, opts: { client?: Store; dryRun?: boolean }): Promise<SyncResult[]> {
  const names = readdirSync(dir).filter((n) => statSync(join(dir, n)).isFile()).sort();
  const out: SyncResult[] = [];
  for (const name of names) out.push(await put(opts.client, join(dir, name), `brand/${name}`, new Uint8Array(readFileSync(join(dir, name))), contentTypeOf(name), { dryRun: opts.dryRun }));
  return out;
}

export async function syncAddressed(kind: string, files: string[], opts: { client?: Store; dryRun?: boolean }): Promise<SyncResult[]> {
  const out: SyncResult[] = [];
  for (const file of files) {
    const bytes = new Uint8Array(readFileSync(file));
    out.push(await put(opts.client, file, addressedKey(kind, bytes, file), bytes, contentTypeOf(file), { dryRun: opts.dryRun, skipExisting: true }));
  }
  return out;
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const cmd = args[0];
  const flag = (name: string, fallback?: string) => {
    const i = args.indexOf(name);
    return i >= 0 ? (args[i + 1] ?? fallback) : fallback;
  };
  const dryRun = args.includes("--dry-run");
  const valueFlags = new Set(["--bucket", "--dir", "--kind", "--prefix"]);
  const positional = args.slice(1).filter((a, i, all) => !a.startsWith("--") && !valueFlags.has(all[i - 1] ?? ""));
  const binding = flag("--bucket", cmd === "assets" && VIDEO_KINDS.has(flag("--kind", "") ?? "") ? "VIDEO" : "ASSETS")!;
  const store = dryRun ? undefined : clientFor(binding);
  let results: SyncResult[];
  if (cmd === "brand") results = await syncBrand(resolve(root, flag("--dir", "brand/assets")!), { client: store, dryRun });
  else if (cmd === "assets") results = await syncAddressed(flag("--kind", "")!, positional, { client: store, dryRun });
  else if (cmd === "mirror") results = await Promise.all(positional.map((u) => mirrorUrlToR2(u, { client: store, prefix: flag("--prefix", "")!, dryRun })));
  else {
    console.error("usage: bun tools/r2-sync.ts brand|assets|mirror [--bucket BINDING] [--dry-run] ... (see the header of the file)");
    process.exit(2);
  }
  console.log(JSON.stringify({ bucket: bucketOf(binding), dryRun, count: results.length, results }, null, 2));
}
