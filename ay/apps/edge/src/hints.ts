// Latency hints of a route, rendered from edge.toml `[services.<name>.hints."<route>"]` into vars.EDGE_ROUTES and applied here:
//   cache         edge cache of cacheable GET responses: Cache API entry for `ttl` seconds, and a `cache-control` with s-maxage and
//                 stale-while-revalidate when the service set none (a service that sets its own cache-control keeps it)
//   early_hints   `Link` values (preload, preconnect) added to the response; with the zone setting early_hints on, Cloudflare caches them and
//                 sends a 103 before the next response is ready
export interface RouteHints {
  cache?: { ttl: number; swr?: number };
  early_hints?: string[];
}

const cacheable = (request: Request, response: Response): boolean =>
  (request.method === "GET" || request.method === "HEAD") && response.status === 200 && !response.headers.has("set-cookie") && !request.headers.has("authorization");

/** Adds the hints of a route to a response; the response is returned as is when it carries nothing to add. */
export function applyHints(request: Request, response: Response, hints: RouteHints | undefined): Response {
  if (!hints) return response;
  const headers = new Headers(response.headers);
  let changed = false;
  if (hints.cache && cacheable(request, response) && !headers.has("cache-control")) {
    const { ttl, swr } = hints.cache;
    headers.set("cache-control", `public, max-age=0, s-maxage=${ttl}${swr ? `, stale-while-revalidate=${swr}` : ""}`);
    changed = true;
  }
  if (hints.early_hints?.length && response.status === 200) {
    const present = new Set(headers.get("link")?.split(/,\s*(?=<)/));
    for (const link of hints.early_hints) {
      if (present.has(link)) continue;
      headers.append("link", link);
      changed = true;
    }
  }
  return changed ? new Response(response.body, { status: response.status, statusText: response.statusText, headers }) : response;
}

/** Serves from the Cache API when the route has a cache hint, else produces the response and stores a cacheable one for `ttl` seconds. */
export async function withEdgeCache(
  request: Request,
  hints: RouteHints | undefined,
  ctx: Pick<ExecutionContext, "waitUntil">,
  produce: () => Promise<Response>,
  store: Cache | undefined = (globalThis as { caches?: { default?: Cache } }).caches?.default,
): Promise<Response> {
  if (!hints?.cache || !store || request.method !== "GET" || request.headers.has("authorization") || request.headers.has("range")) {
    return applyHints(request, await produce(), hints);
  }
  const hit = await store.match(request);
  if (hit) return hit;
  const response = applyHints(request, await produce(), hints);
  if (cacheable(request, response) && response.headers.get("cache-control")?.includes("s-maxage")) {
    ctx.waitUntil(store.put(request, response.clone()));
  }
  return response;
}
