// material.aphrody.com: read-only CDN for Material Symbols SVGs, Google Sans Flex / Google Sans Code WOFF2 and the generated Rust tables.
// Objects live in the R2 bucket bound as MATERIAL under the key `v1/...` (tools/material-cdn builds and publishes the tree).
//
// Cache policy
//   immutable (public, max-age=31536000, immutable):
//     - content-addressed files: `<name>.<sha256 first 8 hex>.woff2`
//     - versioned crate tarballs: `rust/<major>.<minor>.<patch>.tar.gz[.sha256]`
//     - any other file requested with `?v=<sha256 first 8..64 hex>` when the object's SHA-256 matches; a stale pin is a 404, never wrong bytes
//   everything else (index.json, CSS, sprites, stable icon paths without ?v=): short TTL with stale-while-revalidate.

export interface Env {
  MATERIAL: R2Bucket;
}

const IMMUTABLE = "public, max-age=31536000, immutable";
const MUTABLE = "public, max-age=300, s-maxage=3600, stale-while-revalidate=86400";

const MIME: Record<string, string> = {
  svg: "image/svg+xml",
  json: "application/json; charset=utf-8",
  css: "text/css; charset=utf-8",
  woff2: "font/woff2",
  rs: "text/plain; charset=utf-8",
  gz: "application/gzip",
  txt: "text/plain; charset=utf-8",
  sha256: "text/plain; charset=utf-8",
};

const BASE_HEADERS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, HEAD, OPTIONS",
  "Access-Control-Allow-Headers": "Range, If-None-Match, If-Modified-Since",
  "Access-Control-Expose-Headers": "ETag, Content-Length, Content-Range, Accept-Ranges",
  "Cross-Origin-Resource-Policy": "cross-origin",
  "Timing-Allow-Origin": "*",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
};

const HASHED_FONT = /\.[0-9a-f]{8}\.woff2$/;
const VERSIONED_CRATE = /^v1\/rust\/\d+\.\d+\.\d+\.tar\.gz(\.sha256)?$/;
const VALID_KEY = /^v1\/[A-Za-z0-9_\-./]+$/;

function reply(status: number, body: BodyInit | null, extra: Record<string, string> = {}): Response {
  return new Response(body, { status, headers: { ...BASE_HEADERS, ...extra } });
}

async function sha256Hex(buf: ArrayBuffer): Promise<string> {
  const d = new Uint8Array(await crypto.subtle.digest("SHA-256", buf));
  return Array.from(d, (b) => b.toString(16).padStart(2, "0")).join("");
}

async function origin(request: Request, env: Env, key: string, pin: string | null): Promise<Response> {
  const ext = key.slice(key.lastIndexOf(".") + 1);
  const type = MIME[ext] ?? "application/octet-stream";
  const immutableName = HASHED_FONT.test(key) || VERSIONED_CRATE.test(key);
  const headers: Record<string, string> = { "Content-Type": type, "Accept-Ranges": "bytes" };
  if (ext === "svg") headers["Content-Security-Policy"] = "default-src 'none'; style-src 'unsafe-inline'; sandbox";

  if (pin) {
    // Validate the pin against the bytes: cheap for SVGs, once per edge location (the result is cached) for fonts.
    const obj = await env.MATERIAL.get(key);
    if (!obj) return reply(404, "not found\n", { "Cache-Control": "public, max-age=60", "Content-Type": "text/plain; charset=utf-8" });
    const buf = await obj.arrayBuffer();
    const sum = await sha256Hex(buf);
    if (!sum.startsWith(pin)) {
      return reply(404, "pinned hash does not match the current object\n", { "Cache-Control": "no-store", "Content-Type": "text/plain; charset=utf-8" });
    }
    return reply(200, buf, { ...headers, "Cache-Control": IMMUTABLE, ETag: `"${sum}"` });
  }

  const obj = await env.MATERIAL.get(key, { onlyIf: request.headers, range: request.headers });
  if (!obj) return reply(404, "not found\n", { "Cache-Control": "public, max-age=60", "Content-Type": "text/plain; charset=utf-8" });
  headers["Cache-Control"] = immutableName ? IMMUTABLE : MUTABLE;
  headers.ETag = obj.httpEtag;
  headers["Last-Modified"] = obj.uploaded.toUTCString();
  if (!("body" in obj) || obj.body === undefined) return reply(304, null, headers);
  const body = obj as R2ObjectBody;
  let status = 200;
  if (body.range && "offset" in body.range && request.headers.has("Range")) {
    const r = body.range as { offset: number; length: number };
    headers["Content-Range"] = `bytes ${r.offset}-${r.offset + r.length - 1}/${body.size}`;
    headers["Content-Length"] = String(r.length);
    status = 206;
  } else {
    headers["Content-Length"] = String(body.size);
  }
  return reply(status, body.body, headers);
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    if (request.method === "OPTIONS") return reply(204, null, { "Access-Control-Max-Age": "86400" });
    if (request.method !== "GET" && request.method !== "HEAD") return reply(405, "method not allowed\n", { Allow: "GET, HEAD, OPTIONS" });

    if (url.pathname === "/" || url.pathname === "/v1" || url.pathname === "/v1/") {
      return reply(
        200,
        JSON.stringify(
          {
            name: "material.aphrody.com",
            index: "/v1/index.json",
            icons: "/v1/icons/<outlined|rounded|sharp>/[w<100..700>/]<name>[-fill].svg",
            sprites: "/v1/sprites/<outlined|rounded|sharp>[-fill].svg",
            css: ["/v1/fonts/google-sans-flex.css", "/v1/fonts/google-sans-flex-upright.css", "/v1/fonts/google-sans-flex-ui.css", "/v1/fonts/google-sans-code.css", "/v1/fonts/material-symbols.css"],
            rust: ["/v1/rust/material_symbols.rs", "/v1/rust/<version>.tar.gz"],
            licenses: "/v1/licenses/NOTICE.txt",
          },
          null,
          2,
        ) + "\n",
        { "Content-Type": "application/json; charset=utf-8", "Cache-Control": MUTABLE },
      );
    }
    if (url.pathname === "/robots.txt") return reply(200, "User-agent: *\nAllow: /\n", { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": MUTABLE });

    const key = decodeURIComponent(url.pathname).replace(/^\//, "");
    if (!VALID_KEY.test(key) || key.includes("..") || key.endsWith("/")) return reply(404, "not found\n", { "Cache-Control": "public, max-age=60", "Content-Type": "text/plain; charset=utf-8" });
    const v = url.searchParams.get("v");
    const pin = v && /^[0-9a-f]{8,64}$/.test(v) ? v : null;

    // Edge cache keyed by URL (the ?v pin is part of it); HEAD is answered from the GET entry.
    const cacheKey = new Request(url.toString(), { method: "GET" });
    const cache = (caches as unknown as { default: Cache }).default;
    let res = await cache.match(request.method === "GET" ? request : cacheKey);
    if (!res) {
      res = await origin(request.method === "GET" ? request : cacheKey, env, key, pin);
      if (res.status === 200) ctx.waitUntil(cache.put(cacheKey, res.clone()));
    }
    if (request.method === "HEAD") return new Response(null, { status: res.status, headers: res.headers });
    return res;
  },
} satisfies ExportedHandler<Env>;
