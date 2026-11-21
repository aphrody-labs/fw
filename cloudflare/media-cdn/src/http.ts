// Response helpers: one header policy for every route.
export const IMMUTABLE = "public, max-age=31536000, immutable";
/** Names that follow the manifest (no `.<sha8>` pin): an hour at the edge, a few minutes in browsers, revalidated through the ETag. */
export const MUTABLE = "public, max-age=300, s-maxage=3600, stale-while-revalidate=86400";

export const BASE_HEADERS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, HEAD, OPTIONS",
  "Access-Control-Allow-Headers": "Range, If-None-Match, If-Modified-Since, Accept",
  // Timing-Allow-Origin lets pages read transferSize and encodedBodySize of these files in Resource Timing.
  "Access-Control-Expose-Headers": "ETag, Content-Length, Content-Range, Accept-Ranges, Content-Location, Server-Timing, X-Media-Source",
  "Cross-Origin-Resource-Policy": "cross-origin",
  "Timing-Allow-Origin": "*",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
};

export function reply(status: number, body: BodyInit | null, extra: Record<string, string> = {}): Response {
  return new Response(body, { status, headers: { ...BASE_HEADERS, ...extra } });
}

export function problem(status: number, error: string, detail?: unknown): Response {
  return reply(status, JSON.stringify({ error, detail }, null, 2) + "\n", { "Content-Type": "application/json; charset=utf-8", "Cache-Control": status === 404 ? "public, max-age=60" : "no-store" });
}

export async function sha256Hex(data: ArrayBuffer | Uint8Array | string): Promise<string> {
  const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes as BufferSource));
  return Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
}

export const MIME: Record<string, string> = {
  avif: "image/avif",
  webp: "image/webp",
  jpg: "image/jpeg",
  png: "image/png",
  svg: "image/svg+xml",
  mp4: "video/mp4",
  webm: "video/webm",
  json: "application/json; charset=utf-8",
};

/**
 * Serves one R2 object with conditional and range requests. `extra` overrides the defaults (Cache-Control, Vary, Content-Location).
 * Returns undefined when the key does not exist.
 */
export async function serveObject(request: Request, bucket: R2Bucket, key: string, extra: Record<string, string>): Promise<Response | undefined> {
  const ext = key.slice(key.lastIndexOf(".") + 1);
  const headers: Record<string, string> = { "Content-Type": MIME[ext] ?? "application/octet-stream", "Accept-Ranges": "bytes", ...extra };
  if (ext === "svg") headers["Content-Security-Policy"] = "default-src 'none'; style-src 'unsafe-inline'; sandbox";
  const object = await bucket.get(key, { onlyIf: request.headers, range: request.headers });
  if (!object) return undefined;
  headers.ETag = object.httpEtag;
  headers["Last-Modified"] = object.uploaded.toUTCString();
  if (!("body" in object) || object.body === undefined) return reply(304, null, headers);
  const body = object as R2ObjectBody;
  let status = 200;
  const range = body.range as { offset?: number; length?: number; suffix?: number } | undefined;
  if (range && request.headers.has("Range")) {
    const offset = range.offset ?? body.size - (range.suffix ?? 0);
    const length = range.length ?? body.size - offset;
    headers["Content-Range"] = `bytes ${offset}-${offset + length - 1}/${body.size}`;
    headers["Content-Length"] = String(length);
    status = 206;
  } else headers["Content-Length"] = String(body.size);
  return reply(status, body.body, headers);
}

/**
 * Edge cache (Cache API) in front of a producer, for small objects: images and sprites. Range and conditional requests skip it, and
 * `variant` separates the formats a negotiated URL can answer with. A hit says `X-Media-Source: edge-cache`.
 */
export async function cached(request: Request, ctx: ExecutionContext, variant: string, produce: () => Promise<Response | undefined>): Promise<Response | undefined> {
  const plain = request.method === "GET" && !request.headers.has("Range") && !request.headers.has("If-None-Match") && !request.headers.has("If-Modified-Since");
  if (!plain || typeof caches === "undefined") return produce();
  const url = new URL(request.url);
  const key = new Request(`${url.origin}/__edge${url.pathname}?v=${variant}`);
  const hit = await caches.default.match(key);
  if (hit) {
    const again = new Response(hit.body, hit);
    again.headers.set("X-Media-Source", "edge-cache");
    return again;
  }
  const response = await produce();
  if (response?.status === 200) ctx.waitUntil(caches.default.put(key, response.clone()));
  return response;
}
