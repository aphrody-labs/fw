#!/usr/bin/env bun
// Renders the wrangler config of the unified edge Worker from apps/edge/edge.toml and the domains of ay.toml. The route table
// the Worker matches at runtime (vars.EDGE_ROUTES) and the Cloudflare routes are derived from the same list, so they cannot drift.
//   bun tools/edge-config.ts             write apps/edge/wrangler.jsonc (every route; the config of record)
//   bun tools/edge-config.ts --check     fail when the committed file differs from the render (bun run check)
//   bun tools/edge-config.ts --no-routes print the config without routes and custom domains (script upload only, before the cutover)
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { readAy, repoRoot } from "./local.ts";

type Json = Record<string, unknown>;

export interface ServiceSpec {
  mode: "module" | "service";
  script?: string;
  binding?: string;
  enabled?: boolean;
  routes?: string[];
  custom_domains?: string[];
  note?: string;
}

export interface EdgeConfig {
  worker: { name: string; main: string; compatibility_date: string; compatibility_flags?: string[]; cpu_ms: number; preview_workers_dev: boolean };
  domains: Record<string, string>;
  buckets: Record<string, string>;
  services: Record<string, ServiceSpec>;
}

/** One row of the table the Worker matches: host, path (prefix when `prefix`), the service that owns it. */
export interface RouteRow {
  pattern: string;
  host: string;
  path: string;
  prefix: boolean;
  service: string;
  custom: boolean;
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
    for (const raw of spec.routes ?? []) rows.push(parsePattern(expand(raw, cfg.domains), service, false));
    for (const raw of spec.custom_domains ?? []) rows.push(parsePattern(`${expand(raw, cfg.domains)}/*`, service, true));
  }
  return rows;
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
      EDGE_ROUTES: rows.map(({ pattern, host, path, prefix, service }) => ({ pattern, host, path, prefix, service })),
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
    console.log("apps/edge/wrangler.jsonc matches apps/edge/edge.toml");
  } else {
    writeFileSync(target, renderText(cfg));
    console.log(`wrote ${target} (${routeRows(cfg).length} routes, ${Object.keys(cfg.buckets).length} buckets)`);
  }
}
