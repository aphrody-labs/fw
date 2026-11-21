// The R2 bucket behind material.aphrody.com through its S3 API (no REST rate limit). The S3 credentials are derived from the API token as
// Cloudflare documents: access key id = token id, secret = SHA-256(token).
export const TYPES: Record<string, string> = {
  svg: "image/svg+xml",
  json: "application/json; charset=utf-8",
  css: "text/css; charset=utf-8",
  woff2: "font/woff2",
  ttf: "font/ttf",
  rs: "text/plain; charset=utf-8",
  gz: "application/gzip",
  txt: "text/plain; charset=utf-8",
  sha256: "text/plain; charset=utf-8",
  glb: "model/gltf-binary",
  gltf: "model/gltf+json",
  png: "image/png",
  webp: "image/webp",
  avif: "image/avif",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  ktx2: "image/ktx2",
  bin: "application/octet-stream",
};

export interface Bucket {
  token: string;
  accountId: string;
  s3: Bun.S3Client;
  cf: <T>(path: string) => Promise<T>;
}

export async function openBucket(name: string): Promise<Bucket> {
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
    bucket: name,
    region: "auto",
  });
  return { token, accountId, s3, cf };
}

/** R2 answers InternalError / 429 under bursts: retry with exponential backoff and jitter. */
export async function put(bucket: Bucket, key: string, body: Uint8Array | string, type: string) {
  for (let attempt = 0; ; attempt++) {
    try {
      await bucket.s3.write(key, body, { type });
      return;
    } catch (e) {
      if (attempt >= 6) throw e;
      await Bun.sleep(250 * 2 ** attempt + Math.random() * 250);
    }
  }
}

export const typeOf = (key: string) => TYPES[key.split(".").pop()!] ?? "application/octet-stream";
