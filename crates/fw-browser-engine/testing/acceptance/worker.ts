// Cloudflare Worker in front of the dashboard (`bun testing/acceptance.ts serve`), which stays private behind a tunnel.
// The edge terminates TLS and HTTP/3, caches what is cacheable, streams the Server-Sent Events untouched, and adds the
// shared secret the origin checks (ACCEPTANCE_KEY). Bindings: ORIGIN (hostname of the tunnel), ACCEPTANCE_KEY (secret).
import { accessKeys, verifyAccess } from "./access.ts";
import { securityHeaders } from "./policy.ts";

export interface Env {
  ORIGIN: string;
  ACCEPTANCE_KEY: string; // proves the Worker to the origin
  ACCESS_TEAM: string; // Cloudflare Access: who may see the dashboard
  ACCESS_AUD: string;
}

/** Edge cache lifetime per path: the state is a few seconds old at most, screenshots never change, the page rarely does. */
export function ttl(path: string): number {
  if (path === "/api/events") return 0;
  if (path === "/api/state") return 5;
  if (path.startsWith("/shots/")) return 0; // behind Access: never in a shared cache
  return path === "/" ? 60 : 0;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method !== "GET" && request.method !== "HEAD")
      return new Response("method not allowed", { status: 405, headers: { allow: "GET, HEAD" } });
    if (!env.ACCEPTANCE_KEY || !env.ACCESS_TEAM || !env.ACCESS_AUD)
      return new Response("misconfigured: ACCEPTANCE_KEY, ACCESS_TEAM and ACCESS_AUD are required", {
        status: 500,
        headers: { "cache-control": "no-store" },
      });
    const token = request.headers.get("cf-access-jwt-assertion") ?? "";
    if (
      !(await verifyAccess(token, await accessKeys(env.ACCESS_TEAM).catch(() => []), env.ACCESS_TEAM, env.ACCESS_AUD))
    )
      return new Response("forbidden", { status: 403, headers: { "cache-control": "no-store" } });
    const url = new URL(request.url);
    const headers = new Headers({ accept: request.headers.get("accept") ?? "*/*" });
    const last = request.headers.get("last-event-id");
    if (last) headers.set("last-event-id", last);
    headers.set("x-acceptance-key", env.ACCEPTANCE_KEY);
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
    for (const [name, value] of Object.entries(securityHeaders)) out.headers.set(name, value);
    // Everything is behind Access: nothing is public, the edge may keep it for the one who asked.
    out.headers.set("cache-control", seconds ? `private, max-age=${Math.min(seconds, 60)}` : "no-store");
    return out;
  },
};
