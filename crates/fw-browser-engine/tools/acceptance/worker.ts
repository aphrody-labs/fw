// Cloudflare Worker in front of the dashboard (`bun tools/acceptance.ts serve`), which stays private behind a tunnel.
// The edge terminates TLS and HTTP/3, caches what is cacheable, streams the Server-Sent Events untouched, and adds the
// shared secret the origin checks (ACCEPTANCE_KEY). Bindings: ORIGIN (hostname of the tunnel), ACCEPTANCE_KEY (secret).
export interface Env {
  ORIGIN: string;
  ACCEPTANCE_KEY?: string;
}

const policy = [
  "default-src 'none'",
  "script-src 'unsafe-inline'",
  "style-src 'unsafe-inline'",
  "img-src 'self' data:",
  "connect-src 'self'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
].join("; ");

/** Edge cache lifetime per path: the state is a few seconds old at most, screenshots never change, the page rarely does. */
export function ttl(path: string): number {
  if (path === "/api/events") return 0;
  if (path === "/api/state") return 5;
  if (path.startsWith("/shots/")) return 31536000;
  return path === "/" ? 60 : 0;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method !== "GET" && request.method !== "HEAD")
      return new Response("method not allowed", { status: 405, headers: { allow: "GET, HEAD" } });
    const url = new URL(request.url);
    const headers = new Headers({ accept: request.headers.get("accept") ?? "*/*" });
    const last = request.headers.get("last-event-id");
    if (last) headers.set("last-event-id", last);
    if (env.ACCEPTANCE_KEY) headers.set("x-acceptance-key", env.ACCEPTANCE_KEY);
    const seconds = ttl(url.pathname);
    let origin: Response;
    try {
      origin = await fetch(new URL(url.pathname + url.search, `https://${env.ORIGIN}`), {
        method: request.method,
        headers,
        redirect: "manual",
        cf: seconds ? { cacheTtl: seconds, cacheEverything: true } : { cacheTtl: 0 },
      } as RequestInit);
    } catch (error) {
      return Response.json(
        { error: "origin unreachable", detail: (error as Error).message },
        { status: 502, headers: { "cache-control": "no-store" } },
      );
    }
    const out = new Response(origin.body, origin);
    out.headers.set("content-security-policy", policy);
    out.headers.set("x-content-type-options", "nosniff");
    out.headers.set("referrer-policy", "no-referrer");
    if (!out.headers.has("cache-control") || seconds)
      out.headers.set(
        "cache-control",
        seconds ? `public, max-age=${Math.min(seconds, 60)}, s-maxage=${seconds}` : "no-store",
      );
    return out;
  },
};
