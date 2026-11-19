// Serving one R2 object: ranges, conditional requests, ETag, content type and cache policy. Shared by every module of the Worker.

const TYPES: Record<string, string> = {
  json: "application/json; charset=utf-8",
  txt: "text/plain; charset=utf-8",
  sh: "text/x-shellscript; charset=utf-8",
  ps1: "text/plain; charset=utf-8",
  wasm: "application/wasm",
  svg: "image/svg+xml",
  html: "text/html; charset=utf-8",
  zip: "application/zip",
  gz: "application/gzip",
  tgz: "application/gzip",
  dmg: "application/x-apple-diskimage",
  exe: "application/vnd.microsoft.portable-executable",
  sha256: "text/plain; charset=utf-8",
};

export const IMMUTABLE = "public, max-age=31536000, immutable";
export const REVALIDATE = "no-cache";

export function extType(key: string): string | undefined {
  const base = key.slice(key.lastIndexOf("/") + 1);
  if (base === "SHA256SUMS") return TYPES.txt;
  const dot = base.lastIndexOf(".");
  return dot < 0 ? undefined : TYPES[base.slice(dot + 1).toLowerCase()];
}

export const NOT_FOUND = (): Response =>
  new Response("not found", { status: 404, headers: { "cache-control": "no-store", "x-content-type-options": "nosniff" } });

export interface ServeOptions {
  /** Cache-Control of a hit; defaults to immutable. */
  cache?: string;
  /** Content-Disposition: attachment with the file name. */
  download?: boolean;
  cors?: boolean;
}

export async function serveObject(bucket: R2Bucket, key: string, request: Request, opts: ServeOptions = {}): Promise<Response> {
  if (request.method !== "GET" && request.method !== "HEAD") {
    return new Response("method not allowed", { status: 405, headers: { allow: "GET, HEAD" } });
  }
  const rangeHeader = request.headers.get("range");
  const object =
    request.method === "HEAD"
      ? await bucket.head(key)
      : await bucket.get(key, { ...(rangeHeader ? { range: request.headers } : {}), onlyIf: request.headers });
  if (!object) return NOT_FOUND();
  const headers = new Headers();
  object.writeHttpMetadata(headers);
  if (!headers.has("content-type")) headers.set("content-type", extType(key) ?? "application/octet-stream");
  headers.set("etag", object.httpEtag);
  headers.set("accept-ranges", "bytes");
  headers.set("cache-control", opts.cache ?? IMMUTABLE);
  headers.set("x-content-type-options", "nosniff");
  if (opts.cors) {
    headers.set("access-control-allow-origin", "*");
    headers.set("access-control-expose-headers", "etag, content-length, content-range, accept-ranges");
  }
  if (opts.download) headers.set("content-disposition", `attachment; filename="${key.slice(key.lastIndexOf("/") + 1)}"`);
  if (request.method === "HEAD") {
    headers.set("content-length", String(object.size));
    return new Response(null, { status: 200, headers });
  }
  if (!("body" in object)) return new Response(null, { status: 304, headers });
  const body = object as R2ObjectBody;
  headers.set("content-length", String(body.range && "length" in body.range && body.range.length !== undefined ? body.range.length : body.size));
  const partial = !!(rangeHeader && body.range);
  if (partial && body.range && "offset" in body.range) {
    const start = body.range.offset ?? 0;
    const len = body.range.length ?? body.size - start;
    headers.set("content-range", `bytes ${start}-${start + len - 1}/${body.size}`);
    headers.set("content-length", String(len));
  }
  return new Response(body.body, { status: partial ? 206 : 200, headers });
}
