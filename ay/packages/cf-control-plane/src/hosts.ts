import { expandZoneRecords, qualifyHostname, type LocalCloudflareContext } from "./reconcile.ts";
import type { AyCfManifest } from "./schemas.ts";

export interface PublicHost {
  host: string;
  zone: string;
  /** Behind the Cloudflare proxy (orange cloud): the only hosts that terminate HTTP/3 at the edge. */
  proxied: boolean;
  /** What declares it in cf.toml: a proxied DNS record, a Worker route or a Worker custom domain. */
  via: Array<"dns" | "route" | "domain" | "edge">;
}

/** Hostname of a Worker route pattern ("host/path*"); a wildcard host ("*.zone/*") has no single name and yields undefined. */
export function routeHost(pattern: string): string | undefined {
  const host = pattern.split("/")[0]!.toLowerCase();
  return host.includes("*") ? undefined : host;
}

/** Every web-facing host cf.toml declares, once: proxied or DNS-only A, AAAA and CNAME records, Worker routes, Worker custom domains. */
export function publicHosts(manifest: AyCfManifest, local: LocalCloudflareContext = {}): PublicHost[] {
  const out = new Map<string, PublicHost>();
  const add = (host: string, zone: string, proxied: boolean, via: PublicHost["via"][number]) => {
    const prev = out.get(host);
    if (prev) {
      prev.proxied ||= proxied;
      if (!prev.via.includes(via)) prev.via.push(via);
    } else out.set(host, { host, zone, proxied, via: [via] });
  };
  for (const [zone, spec] of Object.entries(manifest.zones)) {
    for (const r of expandZoneRecords(zone, spec, local)) {
      if (r.type === "A" || r.type === "AAAA" || r.type === "CNAME") add(r.name.toLowerCase(), zone, r.proxied, "dns");
    }
    for (const route of spec.worker_routes) {
      const host = routeHost(route.pattern);
      if (host) add(host, zone, true, "route");
    }
    for (const d of spec.worker_domains) add(qualifyHostname(d.hostname, zone).toLowerCase(), zone, true, "domain");
  }
  return [...out.values()].sort((a, b) => a.host.localeCompare(b.host));
}
