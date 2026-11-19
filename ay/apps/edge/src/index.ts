// The unified edge Worker: one script for every host and path of edge.toml. Routing is router.ts; a module service runs here
// (services/), a service-mode service is the script that still owns its code or Durable Object state, reached through its service binding.
import { withEdgeCache } from "./hints.ts";
import { type Resolved, type RouteRow, type ServiceInfo, resolve } from "./router.ts";
import assets from "./services/assets.ts";
import releases from "./services/releases.ts";
import wasm from "./services/wasm.ts";
import type { EdgeEnv, Routed, Service } from "./types.ts";

export const modules: Record<string, Service> = { releases, assets, wasm };

interface Table {
  rows: RouteRow[];
  services: Record<string, ServiceInfo>;
}

const cache = new WeakMap<object, Table>();

/** The route table and the service list rendered into vars by tools/edge-config.ts. */
export function tableOf(env: EdgeEnv): Table {
  const key = env.EDGE_ROUTES as object;
  let t = cache.get(key);
  if (!t) {
    const services = env.EDGE_SERVICES as Record<string, ServiceInfo>;
    t = { rows: env.EDGE_ROUTES as RouteRow[], services };
    cache.set(key, t);
  }
  return t;
}

const json = (body: unknown, status = 200): Response => Response.json(body, { status, headers: { "cache-control": "no-store" } });

/**
 * The URL the services see. Production uses the host of the request. `wrangler dev` rewrites the host to the first route, so a dev
 * session may name a header in vars.EDGE_HOST_HEADER (never rendered into wrangler.jsonc) that carries the host to emulate.
 */
function effectiveUrl(request: Request, env: EdgeEnv): URL {
  const url = new URL(request.url);
  const header = typeof env.EDGE_HOST_HEADER === "string" ? env.EDGE_HOST_HEADER : "";
  const host = header ? request.headers.get(header) : null;
  if (host) url.host = host;
  return url;
}

export async function dispatch(inbound: Request, env: EdgeEnv, ctx: ExecutionContext): Promise<Response> {
  const url = effectiveUrl(inbound, env);
  const request = url.href === inbound.url ? inbound : new Request(url, inbound);
  const { rows, services } = tableOf(env);
  if (url.pathname === "/_/" || url.pathname === "/_") {
    return json({ ok: true, worker: "edge", services, routes: rows.map((r) => ({ pattern: r.pattern, service: r.service })) });
  }
  const hit: Resolved | undefined = resolve(rows, services, url.hostname, url.pathname);
  if (!hit) return json({ error: "no route", host: url.hostname, path: url.pathname }, 404);
  const info = services[hit.service]!;
  if (info.binding) {
    const target = env[info.binding] as Fetcher | undefined;
    if (!target) return json({ error: "service binding missing", service: hit.service, binding: info.binding }, 502);
    if (!hit.preview) return withEdgeCache(request, hit.hints, ctx, () => target.fetch(request));
    const forwarded = new URL(request.url);
    forwarded.pathname = hit.path;
    return target.fetch(new Request(forwarded, request));
  }
  const handler = modules[hit.service];
  if (!handler) return json({ error: "no module for service", service: hit.service }, 501);
  const origin = hit.preview ? `${url.origin}/_/${hit.service}` : url.origin;
  const routed: Routed = { request, url, path: hit.path, origin, env, ctx };
  return withEdgeCache(request, hit.preview ? undefined : hit.hints, ctx, async () => handler(routed));
}

export default {
  fetch: dispatch,
} satisfies ExportedHandler<EdgeEnv>;
