// Host and path routing of the unified Worker. The table is vars.EDGE_ROUTES, rendered by tools/edge-config.ts from edge.toml with the
// same patterns Cloudflare gets as routes, so the Worker and the zone never disagree. The longest matching path wins on a host.

import type { RouteHints } from "./hints.ts";

export interface RouteRow {
  pattern: string;
  host: string;
  path: string;
  prefix: boolean;
  service: string;
  /** Latency hints of the route (edge.toml hints): edge cache and early hints. */
  hints?: RouteHints;
}

export interface ServiceInfo {
  /** Binding name of the service-mode script, null for a module of this Worker. */
  binding: string | null;
}

export interface Resolved {
  service: string;
  /** Path below the mount: the preview prefix is removed, a routed host keeps its full path. */
  path: string;
  preview: boolean;
  hints?: RouteHints;
}

export const PREVIEW_PREFIX = "/_/";

const pathMatches = (row: RouteRow, path: string): boolean => (row.prefix ? path.startsWith(row.path) : path === row.path);

export function matchRoute(rows: readonly RouteRow[], host: string, path: string): RouteRow | undefined {
  const h = host.toLowerCase();
  let best: RouteRow | undefined;
  for (const row of rows) {
    if (row.host !== h || !pathMatches(row, path)) continue;
    if (!best || row.path.length > best.path.length || (row.path.length === best.path.length && best.prefix && !row.prefix)) best = row;
  }
  return best;
}

/**
 * A request on a host of the table goes to the longest route. On any other host (workers.dev, localhost, a preview) the first path
 * segment after /_/ names the service, so every service can be exercised before a route points at the Worker.
 */
export function resolve(rows: readonly RouteRow[], services: Record<string, ServiceInfo>, host: string, path: string): Resolved | undefined {
  const hit = matchRoute(rows, host, path);
  if (hit) return { service: hit.service, path, preview: false, hints: hit.hints };
  const known = rows.some((r) => r.host === host.toLowerCase());
  if (known || !path.startsWith(PREVIEW_PREFIX)) return undefined;
  const rest = path.slice(PREVIEW_PREFIX.length);
  const slash = rest.indexOf("/");
  const service = slash < 0 ? rest : rest.slice(0, slash);
  if (!Object.hasOwn(services, service)) return undefined;
  return { service, path: slash < 0 ? "/" : rest.slice(slash), preview: true };
}
