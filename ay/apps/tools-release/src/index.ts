// Serves the ay-tools Windows builds from R2. /latest.json names the newest build; every other path is a key under ay-tools/ in the bucket.
// A versioned file never changes, so it is cached forever; the pointer is revalidated on every request.
interface Env {
  RELEASES: R2Bucket;
}

const PREFIX = "ay-tools/";

/** The R2 key of a request path, or undefined when the path leaves the ay-tools/ prefix. */
export function keyOf(pathname: string): string | undefined {
  const clean = decodeURIComponent(pathname).replace(/^\/+/, "");
  if (clean === "" || clean.split("/").some((part) => part === ".." || part === "." || part === "")) return undefined;
  return `${PREFIX}${clean}`;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method !== "GET" && request.method !== "HEAD") return new Response("method not allowed", { status: 405 });
    const key = keyOf(new URL(request.url).pathname);
    if (!key) return new Response("not found", { status: 404 });
    const rangeHeader = request.headers.get("range");
    const object = await (request.method === "HEAD"
      ? env.RELEASES.head(key)
      : env.RELEASES.get(key, {
          ...(rangeHeader ? { range: request.headers } : {}),
          onlyIf: request.headers,
        }));
    if (!object) return new Response("not found", { status: 404 });
    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set("etag", object.httpEtag);
    headers.set("accept-ranges", "bytes");
    headers.set("cache-control", key === `${PREFIX}latest.json` ? "no-cache" : "public, max-age=31536000, immutable");
    if (!("body" in object)) return new Response(null, { status: 304, headers });
    return new Response(request.method === "HEAD" ? null : ((object as R2ObjectBody).body as BodyInit), {
      status: rangeHeader && object.range ? 206 : 200,
      headers,
    });
  },
} satisfies ExportedHandler<Env>;
