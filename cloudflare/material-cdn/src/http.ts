// Response helpers shared by the static and the generated paths.
export const IMMUTABLE = "public, max-age=31536000, immutable";
export const MUTABLE = "public, max-age=300, s-maxage=3600, stale-while-revalidate=86400";
/** Unpinned generated output: stable for a day at the edge and in browsers, revalidated through the ETag. */
export const GENERATED = "public, max-age=86400, s-maxage=86400, stale-while-revalidate=604800";

export const MIME: Record<string, string> = {
  svg: "image/svg+xml",
  json: "application/json; charset=utf-8",
  css: "text/css; charset=utf-8",
  woff2: "font/woff2",
  woff: "font/woff",
  ttf: "font/ttf",
  otf: "font/otf",
  rs: "text/plain; charset=utf-8",
  gz: "application/gzip",
  txt: "text/plain; charset=utf-8",
  sha256: "text/plain; charset=utf-8",
  md: "text/markdown; charset=utf-8",
  glb: "model/gltf-binary",
  gltf: "model/gltf+json",
  bin: "application/octet-stream",
  png: "image/png",
  webp: "image/webp",
  avif: "image/avif",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  ico: "image/x-icon",
  ktx2: "image/ktx2",
  mp4: "video/mp4",
  webm: "video/webm",
  ogg: "audio/ogg",
  wasm: "application/wasm",
  js: "text/javascript; charset=utf-8",
  mjs: "text/javascript; charset=utf-8",
};

export const BASE_HEADERS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, HEAD, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Range, If-None-Match, If-Modified-Since, Content-Type",
  "Access-Control-Expose-Headers": "ETag, Content-Length, Content-Range, Accept-Ranges, Content-Location, Server-Timing, X-Material-Source",
  "Cross-Origin-Resource-Policy": "cross-origin",
  "Timing-Allow-Origin": "*",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
};

export function reply(status: number, body: BodyInit | null, extra: Record<string, string> = {}): Response {
  return new Response(body, { status, headers: { ...BASE_HEADERS, ...extra } });
}

export function problem(status: number, error: string, detail?: unknown): Response {
  return reply(status, JSON.stringify({ error, detail }, null, 2) + "\n", { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store" });
}

export async function sha256Hex(data: ArrayBuffer | Uint8Array | string): Promise<string> {
  const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes as BufferSource));
  return Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** True when If-None-Match lists the ETag (weak comparison, `*` matches). */
export function etagMatches(header: string | null, etag: string): boolean {
  if (!header) return false;
  return header.split(",").some((t) => {
    const v = t.trim();
    return v === "*" || v.replace(/^W\//, "") === etag;
  });
}
