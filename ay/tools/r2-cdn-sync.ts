#!/usr/bin/env bun
// Realtime mirror from upstream CDN (e.g. https://dxi4wb638ujep.cloudfront.net) to Cloudflare R2 via Bun.S3Client.
//   bun tools/r2-cdn-sync.ts --bucket aphrody-assets --prefix zukan/aphrody/ <url...> [--dry-run]
import { S3Client } from "bun";

export interface R2SyncConfig {
  bucket: string;
  accountId: string;
  accessKeyId: string;
  secretAccessKey: string;
  prefix?: string;
}

export interface SyncResult {
  sourceUrl: string;
  key: string;
  bytes: number;
  contentType: string;
  sha256: string;
  uploaded: boolean;
}

export function createR2Client(cfg: R2SyncConfig): S3Client {
  return new S3Client({
    accessKeyId: cfg.accessKeyId,
    secretAccessKey: cfg.secretAccessKey,
    bucket: cfg.bucket,
    endpoint: `https://${cfg.accountId}.r2.cloudflarestorage.com`,
  });
}

export function objectKeyForUrl(sourceUrl: string, prefix = ""): string {
  const u = new URL(sourceUrl);
  const cleanPrefix = prefix.replace(/^\/+|\/+$/g, "");
  const cleanPath = u.pathname.replace(/^\/+/, "");
  return cleanPrefix ? `${cleanPrefix}/${cleanPath}` : cleanPath;
}

export async function mirrorUrlToR2(
  sourceUrl: string,
  opts: { client?: S3Client; prefix?: string; dryRun?: boolean; fetcher?: (input: string) => Promise<Response> },
): Promise<SyncResult> {
  const fetcher = opts.fetcher ?? fetch;
  const res = await fetcher(sourceUrl);
  if (!res.ok) throw new Error(`upstream ${res.status} for ${sourceUrl}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  const contentType = res.headers.get("content-type") ?? "application/octet-stream";
  const sha256 = new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
  const key = objectKeyForUrl(sourceUrl, opts.prefix);

  if (!opts.dryRun) {
    if (!opts.client) throw new Error("S3Client required when dryRun is false");
    await opts.client.file(key).write(bytes, { type: contentType });
  }
  return { sourceUrl, key, bytes: bytes.byteLength, contentType, sha256, uploaded: !opts.dryRun };
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const dryRun = args.includes("--dry-run");
  const opt = (flag: string, fallback = "") => {
    const i = args.indexOf(flag);
    return i >= 0 ? (args[i + 1] ?? fallback) : fallback;
  };
  const bucket = opt("--bucket", process.env.R2_BUCKET ?? "aphrody-assets");
  const prefix = opt("--prefix", "zukan/aphrody");
  const urls = args.filter((a) => a.startsWith("https://") || a.startsWith("http://"));
  if (!urls.length) {
    console.error("usage: bun tools/r2-cdn-sync.ts [--bucket aphrody-assets] [--prefix zukan/aphrody] [--dry-run] <url...>");
    process.exit(2);
  }
  const client = dryRun
    ? undefined
    : createR2Client({
        bucket,
        accountId: process.env.CLOUDFLARE_ACCOUNT_ID ?? "",
        accessKeyId: process.env.R2_ACCESS_KEY_ID ?? "",
        secretAccessKey: process.env.R2_SECRET_ACCESS_KEY ?? "",
      });
  const results = await Promise.all(urls.map((u) => mirrorUrlToR2(u, { client, prefix, dryRun })));
  console.log(JSON.stringify({ bucket, dryRun, count: results.length, results }, null, 2));
}
