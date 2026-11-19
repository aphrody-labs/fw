import Cloudflare from "cloudflare";
import type { AyCfManifest, ZoneContract } from "./schemas.ts";
import type { BucketUsageRow } from "./finops.ts";

export interface LocalCloudflareContext {
  cloudflare?: {
    account_id?: string;
    zones?: Record<string, string>;
    tunnels?: Record<string, string>;
  };
  hosts?: {
    build?: { ssh_alias?: string; public_ipv4?: string; public_ipv6?: string };
    edge_proxy?: { ssh_alias?: string; public_ipv4?: string; public_ipv6?: string };
    origin?: { ssh_alias?: string; public_ipv4?: string; public_ipv6?: string };
  };
}

export interface ExpectedDnsRecord {
  type: "A" | "AAAA" | "CNAME" | "MX" | "CAA" | "SRV" | "TXT";
  name: string;
  content: string;
  proxied: boolean;
  priority?: number;
  data?: Record<string, unknown>;
}

export interface ZoneReconcileDiff {
  zone: string;
  expectedDnsCount: number;
  liveDnsCount: number;
  dnsToCreate: ExpectedDnsRecord[];
  dnsToUpdate: Array<{ id: string; record: ExpectedDnsRecord; reason: string }>;
  dnsToDelete: Array<{ id: string; type: string; name: string; content: string }>;
  routesToCreate: Array<{ pattern: string; script: string }>;
  routesToUpdate: Array<{ id: string; pattern: string; script: string }>;
  routesToDelete: Array<{ id: string; pattern: string; script: string }>;
  domainsToCreate: Array<{ hostname: string; service: string }>;
  domainsToDelete: Array<{ id: string; hostname: string; service: string }>;
}

export function qualifyHostname(sub: string, zone: string): string {
  if (sub === "@" || sub === zone) return zone;
  if (sub.endsWith(`.${zone}`)) return sub;
  return `${sub}.${zone}`;
}

export function interpolateHostPlaceholders(value: string, local: LocalCloudflareContext): string {
  const b4 = local.hosts?.build?.public_ipv4 || "192.0.2.1";
  const b6 = local.hosts?.build?.public_ipv6 || "2001:db8::1";
  const e4 = local.hosts?.edge_proxy?.public_ipv4 || "192.0.2.2";
  const e6 = local.hosts?.edge_proxy?.public_ipv6 || "2001:db8::2";
  const o4 = local.hosts?.origin?.public_ipv4 || "192.0.2.3";
  const o6 = local.hosts?.origin?.public_ipv6 || "2001:db8::3";
  return value
    .replaceAll("${host:build:ipv4}", b4)
    .replaceAll("${host:build:ipv6}", b6)
    .replaceAll("${host:edge_proxy:ipv4}", e4)
    .replaceAll("${host:edge_proxy:ipv6}", e6)
    .replaceAll("${host:origin:ipv4}", o4)
    .replaceAll("${host:origin:ipv6}", o6);
}

export function expandZoneRecords(
  zoneName: string,
  spec: ZoneContract,
  local: LocalCloudflareContext = {},
): ExpectedDnsRecord[] {
  const records: ExpectedDnsRecord[] = [];
  const tunnelMap = local.cloudflare?.tunnels ?? {};

  // 1. Direct origins (A / AAAA)
  for (const orig of spec.direct_origins) {
    const role = orig.target.replace(/^host:/, "") as "build" | "edge_proxy" | "origin";
    const hostCfg = local.hosts?.[role];
    const ip =
      orig.type === "A"
        ? (hostCfg?.public_ipv4 || (role === "build" ? "192.0.2.1" : role === "edge_proxy" ? "192.0.2.2" : "192.0.2.3"))
        : (hostCfg?.public_ipv6 || (role === "build" ? "2001:db8::1" : role === "edge_proxy" ? "2001:db8::2" : "2001:db8::3"));
    records.push({
      type: orig.type,
      name: qualifyHostname(orig.name, zoneName),
      content: ip,
      proxied: orig.proxied,
    });
  }

  // 2. Worker AAAA 100::
  for (const sub of spec.worker_aaaa) {
    records.push({
      type: "AAAA",
      name: qualifyHostname(sub, zoneName),
      content: "100::",
      proxied: true,
    });
  }

  // 3. CAA records
  for (const caa of spec.caa_records) {
    records.push({
      type: "CAA",
      name: qualifyHostname(caa.name, zoneName),
      content: `${caa.flags} ${caa.tag} "${caa.value}"`,
      proxied: false,
      data: {
        flags: caa.flags,
        tag: caa.tag,
        value: caa.value,
      },
    });
  }

  // 4. Tunnel CNAMEs
  for (const [tunnelName, subs] of Object.entries(spec.tunnel_cnames)) {
    const uuid = tunnelMap[tunnelName] || `00000000-0000-0000-0000-${tunnelName.padEnd(12, "0").slice(0, 12)}`;
    const target = `${uuid}.cfargotunnel.com`;
    for (const sub of subs) {
      records.push({
        type: "CNAME",
        name: qualifyHostname(sub, zoneName),
        content: target,
        proxied: true,
      });
    }
  }

  // 5. R2 CNAMEs (public.r2.dev)
  for (const [sub] of Object.entries(spec.r2_cnames)) {
    records.push({
      type: "CNAME",
      name: qualifyHostname(sub, zoneName),
      content: "public.r2.dev",
      proxied: true,
    });
  }

  // 6. Static CNAMEs (Mail / DKIM / Resend)
  for (const sc of spec.static_cnames) {
    records.push({
      type: "CNAME",
      name: qualifyHostname(sc.name, zoneName),
      content: sc.target,
      proxied: sc.proxied,
    });
  }

  // 7. MX records
  for (const mx of spec.mx_records) {
    records.push({
      type: "MX",
      name: qualifyHostname(mx.name, zoneName),
      content: mx.target,
      proxied: false,
      priority: mx.priority,
    });
  }

  // 8. SRV records
  for (const srv of spec.srv_records) {
    const fqdn = qualifyHostname(srv.name, zoneName);
    const parts = srv.name.split(".");
    records.push({
      type: "SRV",
      name: fqdn,
      content: `${srv.weight} ${srv.port} ${srv.target}`,
      proxied: false,
      priority: srv.priority,
      data: {
        service: parts[0] ?? "_service",
        proto: parts[1] ?? "_tcp",
        name: parts.length > 2 ? qualifyHostname(parts.slice(2).join("."), zoneName) : zoneName,
        priority: srv.priority,
        weight: srv.weight,
        port: srv.port,
        target: srv.target,
      },
    });
  }

  // 9. TXT records
  for (const txt of spec.txt_records) {
    records.push({
      type: "TXT",
      name: qualifyHostname(txt.name, zoneName),
      content: interpolateHostPlaceholders(txt.content, local),
      proxied: false,
    });
  }

  return records;
}

function dnsMultiKey(r: { type: string; name: string; content: string; priority?: number }): string {
  const t = r.type.toUpperCase();
  const n = r.name.toLowerCase();
  if (t === "MX") return `${t}:${n}:${r.priority ?? 0}:${r.content.toLowerCase()}`;
  if (t === "CAA" || t === "TXT" || t === "SRV") return `${t}:${n}:${r.content}`;
  return `${t}:${n}`;
}

export function diffZoneState(
  zoneName: string,
  spec: ZoneContract,
  liveZone: LiveCloudflareSnapshot["zones"][number],
  local: LocalCloudflareContext = {},
): ZoneReconcileDiff {
  const expectedDns = expandZoneRecords(zoneName, spec, local);
  const dnsToCreate: ExpectedDnsRecord[] = [];
  const dnsToUpdate: Array<{ id: string; record: ExpectedDnsRecord; reason: string }> = [];
  const matchedLiveIds = new Set<string>();

  const liveByMultiKey = new Map<string, Record<string, unknown>>();
  const liveBySingleKey = new Map<string, Array<Record<string, unknown>>>();

  for (const rec of liveZone.dnsRecords) {
    const type = String(rec.type ?? "").toUpperCase();
    const name = String(rec.name ?? "").toLowerCase();
    const content = String(rec.content ?? "");
    const priority = typeof rec.priority === "number" ? rec.priority : undefined;
    const mk = dnsMultiKey({ type, name, content, priority });
    liveByMultiKey.set(mk, rec);
    const sk = `${type}:${name}`;
    const list = liveBySingleKey.get(sk) ?? [];
    list.push(rec);
    liveBySingleKey.set(sk, list);
  }

  for (const exp of expectedDns) {
    const mk = dnsMultiKey(exp);
    const exact = liveByMultiKey.get(mk);
    if (exact) {
      const id = String(exact.id ?? "");
      matchedLiveIds.add(id);
      const liveContent = String(exact.content ?? "");
      const liveProxied = Boolean(exact.proxied);
      if (liveContent !== exp.content || liveProxied !== exp.proxied) {
        dnsToUpdate.push({
          id,
          record: exp,
          reason: `content=${liveContent}->${exp.content} proxied=${liveProxied}->${exp.proxied}`,
        });
      }
      continue;
    }

    const sk = `${exp.type}:${exp.name.toLowerCase()}`;
    const singleCandidates = (liveBySingleKey.get(sk) ?? []).filter((c) => !matchedLiveIds.has(String(c.id ?? "")));
    if ((exp.type === "A" || exp.type === "AAAA" || exp.type === "CNAME") && singleCandidates.length === 1) {
      const target = singleCandidates[0]!;
      const id = String(target.id ?? "");
      matchedLiveIds.add(id);
      dnsToUpdate.push({
        id,
        record: exp,
        reason: `content=${String(target.content)}->${exp.content} proxied=${Boolean(target.proxied)}->${exp.proxied}`,
      });
      continue;
    }

    dnsToCreate.push(exp);
  }

  const dnsToDelete = liveZone.dnsRecords
    .filter((r) => !matchedLiveIds.has(String(r.id ?? "")))
    .map((r) => ({
      id: String(r.id ?? ""),
      type: String(r.type ?? ""),
      name: String(r.name ?? ""),
      content: String(r.content ?? ""),
    }));

  // Worker Routes diff
  const liveRoutesByPattern = new Map<string, { id: string; pattern: string; script: string }>();
  for (const r of liveZone.workerRoutes) {
    const pattern = String(r.pattern ?? "");
    liveRoutesByPattern.set(pattern, {
      id: String(r.id ?? ""),
      pattern,
      script: String(r.script ?? ""),
    });
  }
  const routesToCreate: Array<{ pattern: string; script: string }> = [];
  const routesToUpdate: Array<{ id: string; pattern: string; script: string }> = [];
  const matchedRouteIds = new Set<string>();
  for (const expRoute of spec.worker_routes) {
    const live = liveRoutesByPattern.get(expRoute.pattern);
    if (!live) {
      routesToCreate.push(expRoute);
    } else {
      matchedRouteIds.add(live.id);
      if (live.script !== expRoute.script) {
        routesToUpdate.push({ id: live.id, pattern: expRoute.pattern, script: expRoute.script });
      }
    }
  }
  const routesToDelete = [...liveRoutesByPattern.values()].filter((r) => !matchedRouteIds.has(r.id));

  // Worker Custom Domains diff
  const liveDomainsByHost = new Map<string, { id: string; hostname: string; service: string }>();
  for (const d of liveZone.workerDomains) {
    const hostname = String(d.hostname ?? "").toLowerCase();
    liveDomainsByHost.set(hostname, {
      id: String(d.id ?? ""),
      hostname,
      service: String(d.service ?? ""),
    });
  }
  const domainsToCreate: Array<{ hostname: string; service: string }> = [];
  const matchedDomainIds = new Set<string>();
  for (const expDom of spec.worker_domains) {
    const h = expDom.hostname.toLowerCase();
    const live = liveDomainsByHost.get(h);
    if (!live || live.service !== expDom.service) {
      domainsToCreate.push(expDom);
    } else {
      matchedDomainIds.add(live.id);
    }
  }
  const domainsToDelete = [...liveDomainsByHost.values()].filter((d) => !matchedDomainIds.has(d.id));

  return {
    zone: zoneName,
    expectedDnsCount: expectedDns.length,
    liveDnsCount: liveZone.dnsRecords.length,
    dnsToCreate,
    dnsToUpdate,
    dnsToDelete,
    routesToCreate,
    routesToUpdate,
    routesToDelete,
    domainsToCreate,
    domainsToDelete,
  };
}

export async function applyZoneReconciliation(
  client: Cloudflare,
  manifest: AyCfManifest,
  snapshot: LiveCloudflareSnapshot,
  local: LocalCloudflareContext,
  opts: { apply: boolean; prune?: boolean },
): Promise<{
  zoneDiffs: ZoneReconcileDiff[];
  appliedDnsCreates: number;
  appliedDnsUpdates: number;
  appliedDnsDeletes: number;
  appliedRouteCreates: number;
  appliedRouteUpdates: number;
  appliedRouteDeletes: number;
  appliedDomainCreates: number;
  appliedDomainDeletes: number;
}> {
  const zoneDiffs: ZoneReconcileDiff[] = [];
  let appliedDnsCreates = 0;
  let appliedDnsUpdates = 0;
  let appliedDnsDeletes = 0;
  let appliedRouteCreates = 0;
  let appliedRouteUpdates = 0;
  let appliedRouteDeletes = 0;
  let appliedDomainCreates = 0;
  let appliedDomainDeletes = 0;

  for (const liveZone of snapshot.zones) {
    const spec = manifest.zones[liveZone.name];
    if (!spec) continue;
    const diff = diffZoneState(liveZone.name, spec, liveZone, local);
    zoneDiffs.push(diff);

    if (!opts.apply) continue;

    // 1. Create missing DNS records
    for (const rec of diff.dnsToCreate) {
      await client.dns.records.create({
        zone_id: liveZone.id,
        type: rec.type as never,
        name: rec.name,
        content: rec.content,
        ttl: 1,
        proxied: rec.proxied,
        ...(rec.priority !== undefined ? { priority: rec.priority } : {}),
        ...(rec.data ? { data: rec.data as never } : {}),
      });
      appliedDnsCreates++;
    }

    // 2. Update drifted DNS records (e.g., auto.aphrody.com stale tunnel -> aphrody, tv.dragonballfr.com proxied=true)
    for (const upd of diff.dnsToUpdate) {
      await client.dns.records.edit(upd.id, {
        zone_id: liveZone.id,
        type: upd.record.type as never,
        name: upd.record.name,
        content: upd.record.content,
        ttl: 1,
        proxied: upd.record.proxied,
        ...(upd.record.priority !== undefined ? { priority: upd.record.priority } : {}),
        ...(upd.record.data ? { data: upd.record.data as never } : {}),
      });
      appliedDnsUpdates++;
    }

    // 3. Delete extraneous DNS records when prune is enabled
    if (opts.prune) {
      for (const del of diff.dnsToDelete) {
        await client.dns.records.delete(del.id, { zone_id: liveZone.id });
        appliedDnsDeletes++;
      }
    }

    // 4. Create new Worker Routes BEFORE deleting wildcard/obsolete routes (zero downtime)
    for (const r of diff.routesToCreate) {
      await client.workers.routes.create({
        zone_id: liveZone.id,
        pattern: r.pattern,
        script: r.script,
      });
      appliedRouteCreates++;
    }

    // 5. Update drifted Worker Routes
    for (const r of diff.routesToUpdate) {
      await client.workers.routes.update(r.id, {
        zone_id: liveZone.id,
        pattern: r.pattern,
        script: r.script,
      });
      appliedRouteUpdates++;
    }

    // 6. Delete conflicting/obsolete Worker Routes (e.g., *.aphrody.app/* and redundant R2 root routes)
    for (const r of diff.routesToDelete) {
      await client.workers.routes.delete(r.id, { zone_id: liveZone.id });
      appliedRouteDeletes++;
    }

    // 7. Delete conflicting Worker Custom Domains (e.g., downloads.aphrody.com on aphrody-sites)
    const expAaaaSet = new Set(spec.worker_aaaa.map((sub) => qualifyHostname(sub, liveZone.name).toLowerCase()));
    for (const d of diff.domainsToDelete) {
      await client.workers.domains.delete(d.id, { account_id: snapshot.accountId });
      appliedDomainDeletes++;
      // Deleting a Worker Custom Domain automatically deletes its managed AAAA 100:: record; re-create if declared
      if (expAaaaSet.has(d.hostname.toLowerCase())) {
        await client.dns.records.create({
          zone_id: liveZone.id,
          type: "AAAA",
          name: d.hostname,
          content: "100::",
          ttl: 1,
          proxied: true,
        });
        appliedDnsCreates++;
      }
    }

    // 8. Create missing Worker Custom Domains
    for (const d of diff.domainsToCreate) {
      await client.workers.domains.update({
        account_id: snapshot.accountId,
        zone_id: liveZone.id,
        hostname: d.hostname,
        service: d.service,
      });
      appliedDomainCreates++;
    }
  }

  return {
    zoneDiffs,
    appliedDnsCreates,
    appliedDnsUpdates,
    appliedDnsDeletes,
    appliedRouteCreates,
    appliedRouteUpdates,
    appliedRouteDeletes,
    appliedDomainCreates,
    appliedDomainDeletes,
  };
}

export interface ConflictItem {
  kind:
    | "wildcard-over-r2-custom-domain"
    | "dual-route-and-custom-domain"
    | "stale-tunnel-cname"
    | "unproxied-origin-ip"
    | "tunnel-dns-drift";
  zone: string;
  hostname: string;
  detail: string;
}

export interface LiveCloudflareSnapshot {
  capturedAt: string;
  accountId: string;
  zones: Array<{
    id: string;
    name: string;
    dnsRecords: Array<Record<string, unknown>>;
    workerRoutes: Array<Record<string, unknown>>;
    workerDomains: Array<Record<string, unknown>>;
    cacheRuleset: Record<string, unknown> | null;
  }>;
  workers: Array<{
    id: string;
    compatibilityDate?: string;
    subdomain: { enabled: boolean; previews_enabled: boolean };
    settings: Record<string, unknown>;
    secrets: Array<string>;
  }>;
  buckets: Array<{
    name: string;
    metrics: BucketUsageRow;
    lifecycle: Record<string, unknown>;
    cors: Record<string, unknown>;
    customDomains: Array<Record<string, unknown>>;
    managedR2Dev: boolean;
  }>;
  tunnels: Array<{
    id: string;
    name: string;
    status: string;
    connections: number;
    ingress: Array<Record<string, unknown>>;
  }>;
}

export function createCloudflareClient(apiToken = Bun.env.CLOUDFLARE_API_TOKEN): Cloudflare {
  if (!apiToken) {
    throw new Error("CLOUDFLARE_API_TOKEN is required");
  }
  return new Cloudflare({ apiToken });
}

export async function captureLiveSnapshot(
  client: Cloudflare,
  accountId: string,
): Promise<LiveCloudflareSnapshot> {
  const zonesPage = await client.zones.list({ per_page: 50 });
  const zoneResults = zonesPage.result ?? [];

  const zones = await Promise.all(
    zoneResults.map(async (z) => {
      const [dnsPage, routesRes, domainsRes] = await Promise.all([
        client.dns.records.list({ zone_id: z.id, per_page: 100 }),
        client.workers.routes.list({ zone_id: z.id }),
        client.workers.domains.list({ account_id: accountId, zone_id: z.id }),
      ]);
      let cacheRuleset: Record<string, unknown> | null = null;
      try {
        cacheRuleset = (await client.rulesets.phases.get("http_request_cache_settings", {
          zone_id: z.id,
        })) as unknown as Record<string, unknown>;
      } catch {
        cacheRuleset = null;
      }
      return {
        id: z.id,
        name: z.name,
        dnsRecords: (dnsPage.result ?? []) as unknown as Array<Record<string, unknown>>,
        workerRoutes: (routesRes.result ?? []) as unknown as Array<Record<string, unknown>>,
        workerDomains: (domainsRes.result ?? []) as unknown as Array<Record<string, unknown>>,
        cacheRuleset,
      };
    }),
  );

  const workersPage = await client.workers.scripts.list({ account_id: accountId });
  const workerScripts = workersPage.result ?? [];
  const workers = await Promise.all(
    workerScripts.map(async (w) => {
      const scriptId = String(w.id);
      const [settings, versionSettings, subdomain, secrets] = await Promise.all([
        client.workers.scripts.settings.get(scriptId, { account_id: accountId }),
        client.workers.scripts.scriptAndVersionSettings.get(scriptId, { account_id: accountId }).catch(() => ({
          limits: undefined,
        })),
        client.workers.scripts.subdomain.get(scriptId, { account_id: accountId }).catch(() => ({
          enabled: false,
          previews_enabled: false,
        })),
        client.workers.scripts.secrets.list(scriptId, { account_id: accountId }).catch(() => []),
      ]);
      const mergedSettings = {
        ...(settings as unknown as Record<string, unknown>),
        ...(versionSettings.limits ? { limits: versionSettings.limits } : {}),
      };
      const secretList = Array.isArray(secrets)
        ? secrets
        : ((secrets as { result?: Array<{ name?: string }> }).result ?? []);
      return {
        id: scriptId,
        compatibilityDate: (w as unknown as { compatibility_date?: string }).compatibility_date,
        subdomain: {
          enabled: Boolean((subdomain as { enabled?: boolean }).enabled),
          previews_enabled: Boolean((subdomain as { previews_enabled?: boolean }).previews_enabled),
        },
        settings: mergedSettings,
        secrets: secretList.map((s) => String((s as { name?: string }).name ?? "")).filter(Boolean),
      };
    }),
  );

  const bucketsResp = await client.r2.buckets.list({ account_id: accountId });
  const bucketItems = bucketsResp.buckets ?? [];
  const buckets = await Promise.all(
    bucketItems.map(async (b) => {
      const name = String(b.name);
      const [lifecycle, cors, custom, managed, usageRaw] = await Promise.all([
        client.r2.buckets.lifecycle.get(name, { account_id: accountId }).catch(() => ({ rules: [] })),
        client.r2.buckets.cors.get(name, { account_id: accountId }).catch(() => ({ rules: [] })),
        client.r2.buckets.domains.custom.list(name, { account_id: accountId }).catch(() => ({ domains: [] })),
        client.r2.buckets.domains.managed.list(name, { account_id: accountId }).catch(() => ({ enabled: false })),
        fetch(`https://api.cloudflare.com/client/v4/accounts/${accountId}/r2/buckets/${name}/usage`, {
          headers: { Authorization: `Bearer ${client.apiToken}` },
        })
          .then((r) => r.json())
          .catch(() => ({})),
      ]);
      const usageResult = (usageRaw as { result?: Record<string, unknown> }).result ?? {};
      const payloadBytes = Number(usageResult.payloadSize ?? 0) + Number(usageResult.metadataSize ?? 0);
      const objectCount = Number(usageResult.objectCount ?? 0);
      return {
        name,
        metrics: {
          name,
          tier: "dist" as const,
          bytes: Number.isFinite(payloadBytes) ? payloadBytes : 0,
          objects: Number.isFinite(objectCount) ? objectCount : 0,
        },
        lifecycle: lifecycle as unknown as Record<string, unknown>,
        cors: cors as unknown as Record<string, unknown>,
        customDomains: ((custom as { domains?: Array<Record<string, unknown>> }).domains ?? []),
        managedR2Dev: Boolean((managed as { enabled?: boolean }).enabled),
      };
    }),
  );

  const tunnelsPage = await client.zeroTrust.tunnels.cloudflared.list({
    account_id: accountId,
    is_deleted: false,
  });
  const tunnels = await Promise.all(
    (tunnelsPage.result ?? []).map(async (t) => {
      const id = String(t.id);
      const cfg = await client.zeroTrust.tunnels.cloudflared.configurations
        .get(id, { account_id: accountId })
        .catch(() => null);
      const ingress =
        ((cfg as { config?: { ingress?: Array<Record<string, unknown>> } } | null)?.config?.ingress ?? []);
      return {
        id,
        name: String(t.name ?? id),
        status: String(t.status ?? "unknown"),
        connections: Array.isArray(t.connections) ? t.connections.length : 0,
        ingress,
      };
    }),
  );

  return {
    capturedAt: new Date().toISOString(),
    accountId,
    zones,
    workers,
    buckets,
    tunnels,
  };
}

export function detectStructuralConflicts(snapshot: LiveCloudflareSnapshot): ConflictItem[] {
  const conflicts: ConflictItem[] = [];
  const liveTunnelIds = new Set(snapshot.tunnels.map((t) => t.id));
  const r2DomainsByHost = new Map<string, string>();
  for (const b of snapshot.buckets) {
    for (const d of b.customDomains) {
      const host = String(d.domain ?? "").toLowerCase();
      if (host && d.enabled) r2DomainsByHost.set(host, b.name);
    }
  }

  for (const z of snapshot.zones) {
    const routePatterns = z.workerRoutes.map((r) => ({
      pattern: String(r.pattern ?? ""),
      script: String(r.script ?? ""),
    }));
    const wildcardRoute = routePatterns.find((r) => r.pattern === `*.${z.name}/*`);

    for (const [host, bucket] of r2DomainsByHost) {
      if (host.endsWith(`.${z.name}`) && wildcardRoute) {
        conflicts.push({
          kind: "wildcard-over-r2-custom-domain",
          zone: z.name,
          hostname: host,
          detail: `Wildcard route "${wildcardRoute.pattern}" (${wildcardRoute.script}) intercepts R2 Custom Domain for bucket "${bucket}"`,
        });
      }
    }

    for (const wd of z.workerDomains) {
      const host = String(wd.hostname ?? "").toLowerCase();
      const service = String(wd.service ?? "");
      const matchingRoute = routePatterns.find((r) => r.pattern.toLowerCase().startsWith(`${host}/`));
      if (matchingRoute) {
        conflicts.push({
          kind: "dual-route-and-custom-domain",
          zone: z.name,
          hostname: host,
          detail: `Hostname has BOTH Worker Custom Domain (${service}) and Worker Route (${matchingRoute.pattern} -> ${matchingRoute.script})`,
        });
      }
    }

    for (const rec of z.dnsRecords) {
      const type = String(rec.type ?? "");
      const name = String(rec.name ?? "");
      const content = String(rec.content ?? "");
      const proxied = Boolean(rec.proxied);

      if (type === "CNAME" && content.endsWith(".cfargotunnel.com")) {
        const uuid = content.replace(/\.cfargotunnel\.com$/i, "");
        if (!liveTunnelIds.has(uuid)) {
          conflicts.push({
            kind: "stale-tunnel-cname",
            zone: z.name,
            hostname: name,
            detail: `CNAME points to missing/deleted tunnel ${uuid}`,
          });
        }
      }
      if (type === "A" && !proxied && !/^vps\./i.test(name)) {
        conflicts.push({
          kind: "unproxied-origin-ip",
          zone: z.name,
          hostname: name,
          detail: `Unproxied DNS A record exposes origin IP directly instead of Cloudflare proxy/tunnel`,
        });
      }
    }
  }
  return conflicts;
}

export interface SafeApplySummary {
  subdomainUpdates: Array<{ worker: string; enabled: boolean; previews_enabled: boolean }>;
  observabilityUpdates: Array<{ worker: string; logSampling: number; traceSampling: number; ok: boolean; error?: string }>;
  limitsUpdates: Array<{ worker: string; cpuMs: number; bindingsPreserved: boolean; ok: boolean; error?: string }>;
  r2LifecycleUpdates: Array<{ bucket: string; maxAgeSeconds: number; ok: boolean }>;
  r2CorsUpdates: Array<{ bucket: string; ok: boolean }>;
  r2DevUpdates: Array<{ bucket: string; enabled: boolean; ok: boolean }>;
  cacheRulesetUpdates: Array<{ zone: string; ok: boolean; rulesCount: number }>;
}

export async function applySafeOptimizations(
  client: Cloudflare,
  manifest: AyCfManifest,
  snapshot: LiveCloudflareSnapshot,
  opts: { apply: boolean },
): Promise<SafeApplySummary> {
  const summary: SafeApplySummary = {
    subdomainUpdates: [],
    observabilityUpdates: [],
    limitsUpdates: [],
    r2LifecycleUpdates: [],
    r2CorsUpdates: [],
    r2DevUpdates: [],
    cacheRulesetUpdates: [],
  };

  // 1. Disable exposed .workers.dev / preview URLs on routed production Workers
  for (const w of snapshot.workers) {
    const spec = manifest.workers[w.id];
    if (!spec) continue;
    if (w.subdomain.enabled !== spec.workers_dev || w.subdomain.previews_enabled !== spec.previews) {
      summary.subdomainUpdates.push({
        worker: w.id,
        enabled: spec.workers_dev,
        previews_enabled: spec.previews,
      });
      if (opts.apply) {
        await client.workers.scripts.subdomain.create(w.id, {
          account_id: snapshot.accountId,
          enabled: spec.workers_dev,
          previews_enabled: spec.previews,
        });
      }
    }
  }

  // 2. Apply Observability right-sizing + CPU limits (canary `docs-proxy` & `aphrody-releases` first!)
  const orderedWorkers = [...snapshot.workers].sort((a, b) => {
    const rank = (id: string) => (id === "docs-proxy" ? 0 : id === "aphrody-releases" ? 1 : 2);
    return rank(a.id) - rank(b.id);
  });

  for (const w of orderedWorkers) {
    const spec = manifest.workers[w.id];
    if (!spec) continue;

    const currentObs = (w.settings.observability ?? {}) as {
      enabled?: boolean;
      logs?: { head_sampling_rate?: number };
      traces?: { enabled?: boolean; head_sampling_rate?: number };
    };
    const needsObs =
      currentObs.enabled !== true ||
      currentObs.logs?.head_sampling_rate !== spec.log_sampling ||
      currentObs.traces?.enabled !== true ||
      currentObs.traces?.head_sampling_rate !== spec.trace_sampling;

    if (needsObs) {
      if (!opts.apply) {
        summary.observabilityUpdates.push({
          worker: w.id,
          logSampling: spec.log_sampling,
          traceSampling: spec.trace_sampling,
          ok: true,
        });
      } else {
        try {
          await client.workers.scripts.settings.edit(w.id, {
            account_id: snapshot.accountId,
            observability: {
              enabled: true,
              head_sampling_rate: spec.log_sampling,
              redact_query_string: false,
              logs: {
                enabled: true,
                invocation_logs: true,
                persist: true,
                head_sampling_rate: spec.log_sampling,
              },
              traces: {
                enabled: true,
                persist: true,
                head_sampling_rate: spec.trace_sampling,
              },
            },
          });
          summary.observabilityUpdates.push({
            worker: w.id,
            logSampling: spec.log_sampling,
            traceSampling: spec.trace_sampling,
            ok: true,
          });
        } catch (err) {
          summary.observabilityUpdates.push({
            worker: w.id,
            logSampling: spec.log_sampling,
            traceSampling: spec.trace_sampling,
            ok: false,
            error: (err as Error).message,
          });
        }
      }
    }

    const currentLimits = (w.settings.limits ?? {}) as { cpu_ms?: number };
    if (currentLimits.cpu_ms !== spec.cpu_ms) {
      if (!opts.apply) {
        summary.limitsUpdates.push({
          worker: w.id,
          cpuMs: spec.cpu_ms,
          bindingsPreserved: true,
          ok: true,
        });
      } else {
        try {
          const before = await client.workers.scripts.scriptAndVersionSettings.get(w.id, {
            account_id: snapshot.accountId,
          });
          const beforeBindings = before.bindings ?? [];
          const settingsPart = new File(
            [
              JSON.stringify({
                limits: { cpu_ms: spec.cpu_ms },
                bindings: beforeBindings,
              }),
            ],
            "settings",
            { type: "application/json" },
          );
          await client.workers.scripts.scriptAndVersionSettings.edit(w.id, {
            account_id: snapshot.accountId,
            settings: settingsPart as never,
          });
          const after = await client.workers.scripts.scriptAndVersionSettings.get(w.id, {
            account_id: snapshot.accountId,
          });
          const preserved = (after.bindings?.length ?? 0) === beforeBindings.length;
          summary.limitsUpdates.push({
            worker: w.id,
            cpuMs: spec.cpu_ms,
            bindingsPreserved: preserved,
            ok: preserved,
            ...(preserved ? {} : { error: "bindings count changed after canary edit" }),
          });
          if (!preserved) {
            throw new Error(`Aborting limits update on ${w.id}: bindings mismatch`);
          }
        } catch (err) {
          summary.limitsUpdates.push({
            worker: w.id,
            cpuMs: spec.cpu_ms,
            bindingsPreserved: false,
            ok: false,
            error: (err as Error).message,
          });
          if (w.id === "aphrody-releases" || w.id === "docs-proxy") {
            throw err;
          }
        }
      }
    }
  }

  // 3. Tighten R2 multipart abort lifecycle (604800s -> 86400s) + ensure Range CORS on hot-reusable buckets
  for (const b of snapshot.buckets) {
    const spec = manifest.buckets[b.name];
    if (!spec) continue;
    const rules = ((b.lifecycle as { rules?: Array<Record<string, unknown>> }).rules ?? []).map((r) => ({ ...r }));
    const mpRule = rules.find((r) => Boolean(r.abortMultipartUploadsTransition));
    const currentAge = (
      (mpRule?.abortMultipartUploadsTransition as { condition?: { maxAge?: number } } | undefined)?.condition
        ?.maxAge
    );
    if (currentAge !== spec.abort_multipart_seconds) {
      if (mpRule) {
        mpRule.conditions = {
          prefix: ((mpRule.conditions as { prefix?: string } | undefined)?.prefix ?? ""),
        };
        mpRule.abortMultipartUploadsTransition = {
          condition: { type: "Age", maxAge: spec.abort_multipart_seconds },
        };
      } else {
        rules.push({
          id: "Default Multipart Abort Rule",
          enabled: true,
          conditions: { prefix: "" },
          abortMultipartUploadsTransition: {
            condition: { type: "Age", maxAge: spec.abort_multipart_seconds },
          },
        });
      }
      if (opts.apply) {
        await client.r2.buckets.lifecycle.update(b.name, {
          account_id: snapshot.accountId,
          rules: rules as never,
        });
      }
      summary.r2LifecycleUpdates.push({ bucket: b.name, maxAgeSeconds: spec.abort_multipart_seconds, ok: true });
    }

    if (spec.tier === "hot-reusable") {
      const existingCors = ((b.cors as { rules?: Array<unknown> }).rules ?? []);
      if (existingCors.length === 0) {
        if (opts.apply) {
          await client.r2.buckets.cors.update(b.name, {
            account_id: snapshot.accountId,
            rules: [
              {
                id: "ay-cf-reusable-assets-range",
                allowed: {
                  origins: ["https://aphrody.app", "https://*.aphrody.app", "https://aphrody.com", "http://localhost:*"],
                  methods: ["GET", "HEAD"],
                  headers: ["Range", "If-Range", "Origin", "Accept"],
                },
                exposeHeaders: ["ETag", "Content-Length", "Content-Range", "Accept-Ranges"],
                maxAgeSeconds: 86400,
              },
            ],
          });
        }
        summary.r2CorsUpdates.push({ bucket: b.name, ok: true });
      }
    }

    if (spec.public_r2_dev === false && b.managedR2Dev) {
      if (opts.apply) {
        await client.r2.buckets.domains.managed.update(b.name, {
          account_id: snapshot.accountId,
          enabled: false,
        });
      }
      summary.r2DevUpdates.push({ bucket: b.name, enabled: false, ok: true });
    }
  }

  // 4. Optimize Zone Cache Rulesets cleanly without altering existing origin/site rules
  for (const z of snapshot.zones) {
    const existing = ((z.cacheRuleset as { rules?: Array<Record<string, unknown>> } | null)?.rules ?? []).map(
      (r) => ({ ...r }),
    );
    if (!existing.length) continue;
    let updatedRules: Array<Record<string, unknown>> | null = null;

    if (z.name === "aphrody.com") {
      // Remove duplicate `Imported page rule cdn.aphrody.com/*` and expand primary CDN rule to include releases/downloads/video
      const deduped = existing.filter((r) => r.ref !== "aphrody-pagerule-cdn");
      const target = deduped.find((r) => r.ref === "aphrody-origin-cdn");
      const wantedExpr = '(http.host in {"cdn.aphrody.com" "releases.aphrody.com" "downloads.aphrody.com" "video.aphrody.com"})';
      if (target && (target.expression !== wantedExpr || deduped.length !== existing.length)) {
        target.expression = wantedExpr;
        target.description = "Aphrody CDN, release & media hosts: edge cache following Cache-Control";
        updatedRules = deduped;
      }
    } else if (z.name === "aphrody.app") {
      const target = existing.find((r) => String(r.expression).includes("assets.aphrody.app"));
      const wantedExpr =
        '(http.host in {"assets.aphrody.app" "dist.aphrody.app" "video.aphrody.app" "bun-lib.aphrody.app"})';
      if (target && target.expression !== wantedExpr) {
        target.expression = wantedExpr;
        updatedRules = existing;
      }
    } else if (z.name === "rosegriffon.fr") {
      const target = existing.find((r) => String(r.expression).includes("cdn.rosegriffon.fr"));
      const wantedExpr = '(http.host in {"cdn.rosegriffon.fr" "r2.rosegriffon.fr"})';
      if (target && target.expression !== wantedExpr) {
        target.expression = wantedExpr;
        updatedRules = existing;
      }
    }

    if (updatedRules) {
      const cleanRules = updatedRules.map((r) => ({
        action: r.action,
        action_parameters: r.action_parameters,
        description: r.description,
        enabled: r.enabled,
        expression: r.expression,
        ref: r.ref,
      }));
      if (opts.apply) {
        await client.rulesets.phases.update("http_request_cache_settings", {
          zone_id: z.id,
          rules: cleanRules as never,
        });
      }
      summary.cacheRulesetUpdates.push({ zone: z.name, ok: true, rulesCount: cleanRules.length });
    }
  }

  return summary;
}
