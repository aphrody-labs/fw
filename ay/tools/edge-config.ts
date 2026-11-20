#!/usr/bin/env bun
// Renders the wrangler config of the unified edge Worker from apps/edge/edge.toml and the domains of ay.toml. The route table
// the Worker matches at runtime (vars.EDGE_ROUTES) and the Cloudflare routes are derived from the same list, so they cannot drift.
//   bun tools/edge-config.ts             write apps/edge/wrangler.jsonc (every route; the config of record)
//   bun tools/edge-config.ts --check     fail when the committed file differs from the render, or when cf.toml, edge.toml and the rendered
//                                        routes disagree (checkEdge below); run by bun run check
//   bun tools/edge-config.ts --no-routes print the config without routes and custom domains (script upload only, before the cutover)
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expandZoneRecords, type AyCfManifest, type LocalCloudflareContext } from "../packages/cf-control-plane/src/index.ts";
import { AyCfManifestSchema } from "../packages/cf-control-plane/src/schemas.ts";
import { readAy, repoRoot } from "./local.ts";

type Json = Record<string, unknown>;

/** Latency hints of one route, applied by apps/edge/src/hints.ts (docs/edge.md, Latency). */
export interface RouteHints {
  cache?: { ttl: number; swr?: number };
  early_hints?: string[];
}

export interface ServiceSpec {
  mode: "module" | "service";
  script?: string;
  binding?: string;
  enabled?: boolean;
  routes?: string[];
  custom_domains?: string[];
  /** Bucket service: the R2 binding, CORS rule and fallback Cache-Control of each host (a host written as in `routes`, templates allowed). */
  hosts?: Record<string, { bucket: string; cors?: string; cache?: string }>;
  /** Hints by route template exactly as written in `routes` or `custom_domains`. */
  hints?: Record<string, RouteHints>;
  note?: string;
}

export interface EdgeConfig {
  worker: { name: string; main: string; compatibility_date: string; compatibility_flags?: string[]; cpu_ms: number; preview_workers_dev: boolean; placement?: "smart" };
  domains: Record<string, string>;
  buckets: Record<string, string>;
  services: Record<string, ServiceSpec>;
  /** CORS rules of the bucket hosts by name; origins may use {root}, {app}, {shenron}. */
  cors?: Record<string, { origins: string[]; methods?: string[]; headers?: string[]; expose?: string[]; max_age?: number }>;
}

/** One row of the table the Worker matches: host, path (prefix when `prefix`), the service that owns it. */
export interface RouteRow {
  pattern: string;
  host: string;
  path: string;
  prefix: boolean;
  service: string;
  custom: boolean;
  hints?: RouteHints;
}

export const edgeDir = join(repoRoot, "apps", "edge");

export function loadEdgeConfig(root = repoRoot): EdgeConfig {
  const cfg = Bun.TOML.parse(readFileSync(join(root, "apps", "edge", "edge.toml"), "utf8")) as unknown as EdgeConfig;
  const ay = readAy(root) as { domains?: Record<string, string> };
  cfg.domains = { ...(ay.domains ?? {}), ...cfg.domains };
  return cfg;
}

/** Replaces `{key}` by the domain of that key; an unknown key is an error, never a silent hole. */
export function expand(template: string, domains: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_, k: string) => {
    const v = domains[k];
    if (!v) throw new Error(`edge.toml: {${k}} is not a domain of ay.toml [domains] or edge.toml [domains]`);
    return v;
  });
}

export function enabledServices(cfg: EdgeConfig): [string, ServiceSpec][] {
  return Object.entries(cfg.services).filter(([, s]) => s.enabled !== false);
}

export function routeRows(cfg: EdgeConfig): RouteRow[] {
  const rows: RouteRow[] = [];
  for (const [service, spec] of enabledServices(cfg)) {
    for (const raw of spec.routes ?? []) rows.push(withHints(parsePattern(expand(raw, cfg.domains), service, false), spec.hints?.[raw], cfg));
    for (const raw of spec.custom_domains ?? []) rows.push(withHints(parsePattern(`${expand(raw, cfg.domains)}/*`, service, true), spec.hints?.[raw], cfg));
  }
  return rows;
}

function withHints(row: RouteRow, hints: RouteHints | undefined, cfg: EdgeConfig): RouteRow {
  if (!hints) return row;
  const out: RouteHints = {};
  if (hints.cache) out.cache = hints.cache;
  if (hints.early_hints?.length) out.early_hints = hints.early_hints.map((h) => expand(h, cfg.domains));
  return Object.keys(out).length ? { ...row, hints: out } : row;
}

export function parsePattern(pattern: string, service: string, custom: boolean): RouteRow {
  const slash = pattern.indexOf("/");
  const host = (slash < 0 ? pattern : pattern.slice(0, slash)).toLowerCase();
  const rest = slash < 0 ? "/*" : pattern.slice(slash);
  const prefix = rest.endsWith("*");
  const path = prefix ? rest.slice(0, -1) : rest;
  return { pattern, host, path, prefix, service, custom };
}

/** Zone of a host: the longest value of [domains] it equals or is a subdomain of. */
export function zoneOf(host: string, domains: Record<string, string>): string {
  const zones = Object.values(domains).filter((z) => host === z || host.endsWith(`.${z}`));
  if (!zones.length) throw new Error(`no zone in ay.toml/edge.toml [domains] covers ${host}`);
  return zones.sort((a, b) => b.length - a.length)[0]!;
}

/** vars.EDGE_BUCKETS and vars.EDGE_CORS of the bucket service, absent when edge.toml has none. */
export function bucketVars(cfg: EdgeConfig): Json {
  const hosts = enabledServices(cfg).flatMap(([, s]) => Object.entries(s.hosts ?? {}));
  if (!hosts.length) return {};
  const used = new Set(hosts.flatMap(([, h]) => (h.cors ? [h.cors] : [])));
  return {
    EDGE_BUCKETS: Object.fromEntries(hosts.map(([host, h]) => [expand(host, cfg.domains).toLowerCase(), { bucket: h.bucket, ...(h.cors ? { cors: h.cors } : {}), ...(h.cache ? { cache: h.cache } : {}) }])),
    EDGE_CORS: Object.fromEntries(
      [...used].map((name) => {
        const r = cfg.cors?.[name];
        if (!r) throw new Error(`edge.toml: cors rule ${name} is not declared in [cors.${name}]`);
        return [name, { origins: r.origins.map((o) => expand(o, cfg.domains)), methods: r.methods ?? ["GET", "HEAD"], headers: r.headers ?? [], expose: r.expose ?? [], max_age: r.max_age ?? 86400 }];
      }),
    ),
  };
}

export function renderWrangler(cfg: EdgeConfig, opts: { routes?: boolean } = {}): Json {
  const withRoutes = opts.routes !== false;
  const rows = routeRows(cfg);
  const out: Json = {
    $schema: "../../node_modules/wrangler/config-schema.json",
    name: cfg.worker.name,
    main: cfg.worker.main,
    compatibility_date: cfg.worker.compatibility_date,
    ...(cfg.worker.compatibility_flags?.length ? { compatibility_flags: cfg.worker.compatibility_flags } : {}),
    workers_dev: !withRoutes && cfg.worker.preview_workers_dev,
    preview_urls: false,
    limits: { cpu_ms: cfg.worker.cpu_ms },
    ...(cfg.worker.placement ? { placement: { mode: cfg.worker.placement } } : {}),
    observability: {
      enabled: true,
      head_sampling_rate: 0.1,
      logs: { enabled: true, invocation_logs: true, head_sampling_rate: 0.1 },
      traces: { enabled: true, head_sampling_rate: 0.05 },
    },
    rules: [{ type: "CompiledWasm", globs: ["**/*.wasm"], fallthrough: true }],
    r2_buckets: Object.entries(cfg.buckets).map(([binding, bucket_name]) => ({ binding, bucket_name })),
    services: enabledServices(cfg)
      .filter(([, s]) => s.mode === "service")
      .map(([name, s]) => {
        if (!s.script || !s.binding) throw new Error(`edge.toml: service ${name} needs script and binding`);
        return { binding: s.binding, service: s.script };
      }),
    vars: {
      EDGE_SERVICES: Object.fromEntries(enabledServices(cfg).map(([n, s]) => [n, { binding: s.mode === "service" ? s.binding : null }])),
      ...bucketVars(cfg),
      EDGE_ROUTES: rows.map(({ pattern, host, path, prefix, service, hints }) => ({ pattern, host, path, prefix, service, ...(hints ? { hints } : {}) })),
    },
  };
  if (withRoutes) {
    out.routes = rows.map((r) =>
      r.custom ? { pattern: r.host, custom_domain: true } : { pattern: r.pattern, zone_name: zoneOf(r.host, cfg.domains) },
    );
  }
  return out;
}

export function renderText(cfg: EdgeConfig, opts: { routes?: boolean } = {}): string {
  return JSON.stringify(renderWrangler(cfg, opts), null, 2) + "\n";
}

export interface EdgeProblem {
  where: string;
  message: string;
}

/**
 * Drift between cf.toml (what the zones hold), edge.toml (what the Worker owns) and the rendered wrangler config:
 *  - every Worker route and custom domain of cf.toml is either owned by the edge Worker (its script is the Worker name and edge.toml has
 *    that exact route) or intentionally external (its script is in cf.toml `external_scripts` and edge.toml has no such route): exactly one;
 *  - every edge.toml route is declared in cf.toml, in a zone cf.toml knows, and the host has a proxied DNS record or is a custom domain;
 *  - a route pattern belongs to one service; hints name a route of their service;
 *  - the Worker contract of cf.toml ([workers.<name>]) agrees with [worker] of edge.toml (cpu_ms) and workers_dev is off once routed;
 *  - the committed wrangler.jsonc carries exactly the routes and table of the render.
 */
export function checkEdge(cfg: EdgeConfig, manifest: AyCfManifest, wrangler: string, local: LocalCloudflareContext = {}): EdgeProblem[] {
  const problems: EdgeProblem[] = [];
  const bad = (where: string, message: string) => problems.push({ where, message });
  const worker = cfg.worker.name;
  const rows = routeRows(cfg);
  const external = new Set(manifest.edge.external_scripts);

  const seen = new Map<string, string>();
  for (const r of rows) {
    const key = `${r.custom ? "domain" : "route"} ${r.pattern}`;
    if (seen.has(key)) bad("edge.toml", `${r.pattern} is declared by services ${seen.get(key)} and ${r.service}`);
    seen.set(key, r.service);
  }
  for (const [name, spec] of Object.entries(cfg.services)) {
    for (const [host, h] of Object.entries(spec.hosts ?? {})) {
      if (!(spec.routes ?? []).some((rt) => rt.startsWith(`${host}/`))) bad(`edge.toml services.${name}.hosts`, `"${host}" is not the host of a route of the service`);
      if (!(h.bucket in cfg.buckets)) bad(`edge.toml services.${name}.hosts.${host}`, `bucket ${h.bucket} is not in [buckets]`);
      if (h.cors && !cfg.cors?.[h.cors]) bad(`edge.toml services.${name}.hosts.${host}`, `cors ${h.cors} is not in [cors]`);
    }
    for (const key of Object.keys(spec.hints ?? {})) {
      if (![...(spec.routes ?? []), ...(spec.custom_domains ?? [])].includes(key)) bad(`edge.toml services.${name}.hints`, `"${key}" is not a route of the service`);
    }
  }

  const zoneNames = Object.keys(manifest.zones);
  const declared = new Set<string>();
  for (const [zone, spec] of Object.entries(manifest.zones)) {
    for (const route of spec.worker_routes) {
      const owner = edgeOwner(rows, route.pattern, false);
      declared.add(`route ${route.pattern}`);
      classify(`cf.toml zones.${zone}.worker_routes ${route.pattern}`, route.script, owner);
    }
    for (const domain of spec.worker_domains) {
      const host = domain.hostname.toLowerCase();
      const owner = edgeOwner(rows, host, true);
      declared.add(`domain ${host}`);
      classify(`cf.toml zones.${zone}.worker_domains ${host}`, domain.service, owner);
    }
  }
  function classify(where: string, script: string, owner: RouteRow | undefined) {
    if (script === worker) {
      if (!owner) bad(where, `script ${script} is the edge Worker but edge.toml has no such route`);
    } else if (external.has(script)) {
      if (owner) bad(where, `script ${script} is external but edge.toml routes it to service ${owner.service}: owned twice`);
    } else bad(where, `script ${script} is neither the edge Worker ${worker} nor listed in cf.toml external_scripts`);
  }

  for (const r of rows) {
    if (!declared.has(`${r.custom ? "domain" : "route"} ${r.custom ? r.host : r.pattern}`)) bad(`edge.toml ${r.pattern}`, "not declared in cf.toml worker_routes or worker_domains");
    let zone: string;
    try {
      zone = zoneOf(r.host, cfg.domains);
    } catch (e) {
      bad(`edge.toml ${r.pattern}`, (e as Error).message);
      continue;
    }
    if (!zoneNames.includes(zone)) bad(`edge.toml ${r.pattern}`, `zone ${zone} is not in cf.toml zones`);
    else if (!r.custom && !proxiedHosts(zone).has(r.host)) bad(`edge.toml ${r.pattern}`, `${r.host} has no proxied DNS record in cf.toml zone ${zone}: the route never receives traffic`);
  }
  function proxiedHosts(zone: string): Set<string> {
    return new Set(expandZoneRecords(zone, manifest.zones[zone]!, local).filter((x) => x.proxied && ["A", "AAAA", "CNAME"].includes(x.type)).map((x) => x.name.toLowerCase()));
  }

  const contract = manifest.workers[worker];
  if (!contract) bad("cf.toml workers", `no [workers.${worker}] contract for the edge Worker`);
  else {
    if (contract.cpu_ms !== cfg.worker.cpu_ms) bad(`cf.toml workers.${worker}`, `cpu_ms ${contract.cpu_ms} differs from edge.toml [worker] cpu_ms ${cfg.worker.cpu_ms}`);
    if (contract.workers_dev) bad(`cf.toml workers.${worker}`, "workers_dev must be false: the routed Worker answers on its zones only");
  }

  let parsed: Json | undefined;
  try {
    parsed = JSON.parse(wrangler) as Json;
  } catch {
    bad("apps/edge/wrangler.jsonc", "is not valid JSON");
  }
  if (parsed) {
    const want = renderWrangler(cfg);
    for (const key of ["routes", "vars", "placement", "limits", "workers_dev"]) {
      if (JSON.stringify(parsed[key]) !== JSON.stringify(want[key])) bad("apps/edge/wrangler.jsonc", `${key} differs from the render of edge.toml`);
    }
    const files = ((parsed.routes ?? []) as Array<{ pattern: string; zone_name?: string; custom_domain?: boolean }>).filter((r) => r.zone_name && !zoneNames.includes(r.zone_name));
    for (const r of files) bad("apps/edge/wrangler.jsonc", `route ${r.pattern} names zone ${r.zone_name}, which cf.toml does not know`);
  }
  return problems;
}

function edgeOwner(rows: RouteRow[], key: string, custom: boolean): RouteRow | undefined {
  return rows.find((r) => r.custom === custom && (custom ? r.host === key : r.pattern === key));
}

export function loadCfManifest(root = repoRoot): AyCfManifest {
  return AyCfManifestSchema.parse(Bun.TOML.parse(readFileSync(join(root, "packages", "cf-control-plane", "cf.toml"), "utf8")));
}

export function loadCfLocal(root = repoRoot): LocalCloudflareContext {
  try {
    return Bun.TOML.parse(readFileSync(join(root, "cf.local.toml"), "utf8")) as LocalCloudflareContext;
  } catch {
    return {};
  }
}

/** Binding of every enabled service-mode service, by service name (used by the router to find its Fetcher). */
export function serviceBindings(cfg: EdgeConfig): Record<string, string> {
  return Object.fromEntries(enabledServices(cfg).filter(([, s]) => s.mode === "service").map(([n, s]) => [n, s.binding!]));
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const cfg = loadEdgeConfig();
  const target = join(edgeDir, "wrangler.jsonc");
  if (args.includes("--no-routes")) {
    process.stdout.write(renderText(cfg, { routes: false }));
  } else if (args.includes("--check")) {
    let current = "";
    try {
      current = readFileSync(target, "utf8");
    } catch {}
    if (current.replaceAll("\r\n", "\n") !== renderText(cfg)) {
      console.error("apps/edge/wrangler.jsonc is stale: run `bun tools/edge-config.ts`");
      process.exit(1);
    }
    const problems = checkEdge(cfg, loadCfManifest(), current, loadCfLocal());
    for (const p of problems) console.error(`${p.where}: ${p.message}`);
    if (problems.length) process.exit(1);
    console.log("apps/edge/wrangler.jsonc matches apps/edge/edge.toml; cf.toml, edge.toml and the rendered routes agree");
  } else {
    writeFileSync(target, renderText(cfg));
    console.log(`wrote ${target} (${routeRows(cfg).length} routes, ${Object.keys(cfg.buckets).length} buckets)`);
  }
}
