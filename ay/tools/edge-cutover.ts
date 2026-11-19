#!/usr/bin/env bun
// Moves the live routes of the unified edge Worker from the scripts that own them today to the Worker, one route at a time, and back.
// The Worker itself is uploaded first without routes (`bun tools/edge-config.ts --no-routes`, wrangler deploy), checked on its workers.dev
// preview, and only then do the routes move: a route owned by another script cannot be declared by a second deploy, and re-pointing it is
// instant and reversible. Routes and custom domains come from apps/edge/edge.toml; the account and zone ids from cf.local.toml
// (git-ignored) or CLOUDFLARE_ACCOUNT_ID; the token is CLOUDFLARE_API_TOKEN (never printed).
//   bun tools/edge-cutover.ts plan [--json]    read-only: the owner of every route today and the action each would take
//   bun tools/edge-cutover.ts apply --apply    re-point (or create) every route to the Worker; previous owners go to .private/edge-cutover.json
//   bun tools/edge-cutover.ts rollback --apply re-point every route back to its previous owner from that file (a created route is deleted)
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { enabledServices, loadEdgeConfig, routeRows, zoneOf, type EdgeConfig, type RouteRow } from "./edge-config.ts";
import { privatePath, repoRoot } from "./local.ts";

export interface LiveRoute {
  id: string;
  pattern: string;
  script?: string | null;
}
export interface LiveDomain {
  hostname: string;
  service: string;
  zone_id?: string;
}

export type Action = "create" | "repoint" | "keep";
export interface Step {
  kind: "route" | "domain";
  pattern: string;
  zone: string;
  /** Script that owns it today, null when it does not exist yet. */
  from: string | null;
  routeId?: string;
  to: string;
  action: Action;
}

export interface Live {
  routes: Record<string, LiveRoute[]>;
  domains: LiveDomain[];
  scripts: string[];
  /** Hosts of routes that have no proxied DNS record in their zone: a route there never receives traffic. */
  unproxied: string[];
}

/** What it takes to make every row of the table belong to the Worker. A route that already belongs to it is kept. */
export function plan(cfg: EdgeConfig, rows: RouteRow[], live: Live): Step[] {
  const to = cfg.worker.name;
  return rows.map((row): Step => {
    const zone = zoneOf(row.host, cfg.domains);
    if (row.custom) {
      const cur = live.domains.find((d) => d.hostname === row.host);
      return { kind: "domain", pattern: row.host, zone, from: cur?.service ?? null, to, action: !cur ? "create" : cur.service === to ? "keep" : "repoint" };
    }
    const cur = (live.routes[zone] ?? []).find((r) => r.pattern === row.pattern);
    return { kind: "route", pattern: row.pattern, zone, from: cur?.script ?? null, routeId: cur?.id, to, action: !cur ? "create" : cur.script === to ? "keep" : "repoint" };
  });
}

/** Service-mode scripts the Worker binds to that do not exist on the account: the first deploy would fail on them. */
export function missingScripts(cfg: EdgeConfig, live: Live): string[] {
  return enabledServices(cfg).flatMap(([, s]) => (s.mode === "service" && s.script && !live.scripts.includes(s.script) ? [s.script] : []));
}

export type Api = (path: string, init?: { method?: string; body?: unknown }) => Promise<{ success: boolean; result?: unknown; errors?: unknown }>;

export function cloudflareApi(token: string, fetcher: typeof fetch = fetch): Api {
  return async (path, init) => {
    const res = await fetcher(`https://api.cloudflare.com/client/v4/${path}`, {
      method: init?.method ?? "GET",
      headers: { authorization: `Bearer ${token}`, ...(init?.body ? { "content-type": "application/json" } : {}) },
      body: init?.body ? JSON.stringify(init.body) : undefined,
    });
    return (await res.json()) as { success: boolean; result?: unknown; errors?: unknown };
  };
}

export interface Local {
  accountId: string;
  zones: Record<string, string>;
}

/** Account and zone ids of the git-ignored cf.local.toml, with the environment taking precedence for the account. */
export function readLocal(root = repoRoot): Local {
  const file = join(root, "cf.local.toml");
  const cf = (existsSync(file) ? (Bun.TOML.parse(readFileSync(file, "utf8")) as { cloudflare?: { account_id?: string; zones?: Record<string, string> } }).cloudflare : undefined) ?? {};
  const accountId = process.env.CLOUDFLARE_ACCOUNT_ID ?? cf.account_id;
  if (!accountId) throw new Error("no account id: set CLOUDFLARE_ACCOUNT_ID or [cloudflare].account_id in cf.local.toml");
  return { accountId, zones: cf.zones ?? {} };
}

async function ok<T>(api: Api, path: string, init?: { method?: string; body?: unknown }): Promise<T> {
  const r = await api(path, init);
  if (!r.success) throw new Error(`${init?.method ?? "GET"} ${path}: ${JSON.stringify(r.errors)}`);
  return r.result as T;
}

export async function readLive(api: Api, cfg: EdgeConfig, local: Local): Promise<Live> {
  const zones = [...new Set(routeRows(cfg).filter((r) => !r.custom).map((r) => zoneOf(r.host, cfg.domains)))];
  const routes: Live["routes"] = {};
  for (const zone of zones) {
    const id = local.zones[zone];
    if (!id) throw new Error(`cf.local.toml [cloudflare.zones] has no id for ${zone}`);
    routes[zone] = await ok<LiveRoute[]>(api, `zones/${id}/workers/routes`);
  }
  const domains = await ok<LiveDomain[]>(api, `accounts/${local.accountId}/workers/domains`);
  const scripts = (await ok<Array<{ id: string }>>(api, `accounts/${local.accountId}/workers/scripts`)).map((s) => s.id);
  const unproxied: string[] = [];
  for (const host of new Set(routeRows(cfg).filter((r) => !r.custom).map((r) => r.host))) {
    const zone = zoneOf(host, cfg.domains);
    const records = await ok<Array<{ proxied?: boolean; type: string }>>(api, `zones/${local.zones[zone]}/dns_records?name=${encodeURIComponent(host)}`);
    if (!records.some((r) => r.proxied)) unproxied.push(host);
  }
  return { routes, domains, scripts, unproxied };
}

export interface Saved {
  worker: string;
  date: string;
  steps: Step[];
}

async function apply(api: Api, local: Local, steps: Step[], direction: "forward" | "back"): Promise<string[]> {
  const log: string[] = [];
  for (const s of steps) {
    const zoneId = local.zones[s.zone];
    const target = direction === "forward" ? s.to : s.from;
    if (s.action === "keep") continue;
    if (s.kind === "route") {
      if (!zoneId) throw new Error(`no zone id for ${s.zone}`);
      if (direction === "back" && s.action === "create") {
        await ok(api, `zones/${zoneId}/workers/routes/${s.routeId}`, { method: "DELETE" });
        log.push(`deleted route ${s.pattern}`);
      } else if (s.action === "create") {
        const created = await ok<{ id: string }>(api, `zones/${zoneId}/workers/routes`, { method: "POST", body: { pattern: s.pattern, script: target } });
        s.routeId = created?.id ?? s.routeId;
        log.push(`created route ${s.pattern} -> ${target}`);
      } else {
        await ok(api, `zones/${zoneId}/workers/routes/${s.routeId}`, { method: "PUT", body: { pattern: s.pattern, script: target } });
        log.push(`route ${s.pattern} -> ${target}`);
      }
    } else if (direction === "back" && s.action === "create") {
      log.push(`custom domain ${s.pattern} was created by the cutover: detach it from ${s.to} in the dashboard (Workers custom domains)`);
    } else {
      await ok(api, `accounts/${local.accountId}/workers/domains`, { method: "PUT", body: { hostname: s.pattern, service: target, zone_id: zoneId, environment: "production" } });
      log.push(`custom domain ${s.pattern} -> ${target}`);
    }
  }
  return log;
}

export { apply as applySteps };

export function render(steps: Step[], missing: string[], unproxied: string[] = []): string {
  const rows = steps.map((s) => `${s.action.padEnd(8)} ${s.kind.padEnd(6)} ${s.pattern.padEnd(40)} ${(s.from ?? "(none)").padEnd(18)} -> ${s.to}`);
  return [
    "action   kind   pattern                                  from               -> to",
    ...rows,
    ...(missing.length ? ["", `missing service scripts (bindings would fail the deploy): ${missing.join(", ")}`] : []),
    ...(unproxied.length ? ["", `no proxied DNS record, add one before the route can serve: ${unproxied.join(", ")}`] : []),
  ].join("\n");
}

if (import.meta.main) {
  const [cmd = "plan", ...rest] = Bun.argv.slice(2);
  const cfg = loadEdgeConfig();
  const local = readLocal();
  const token = process.env.CLOUDFLARE_API_TOKEN;
  if (!token) throw new Error("CLOUDFLARE_API_TOKEN is not set");
  const api = cloudflareApi(token);
  const state = privatePath("edge-cutover.json", repoRoot, true);
  const go = rest.includes("--apply");
  if (cmd === "plan" || cmd === "apply") {
    const live = await readLive(api, cfg, local);
    const steps = plan(cfg, routeRows(cfg), live);
    const missing = missingScripts(cfg, live);
    if (cmd === "plan" || !go) {
      console.log(rest.includes("--json") ? JSON.stringify({ steps, missing, unproxied: live.unproxied }, null, 2) : render(steps, missing, live.unproxied));
      if (cmd === "apply") console.log("\ndry run: add --apply to move the routes");
    } else {
      if (!live.scripts.includes(cfg.worker.name)) throw new Error(`${cfg.worker.name} is not deployed: upload it first (wrangler deploy with the no-routes config)`);
      await Bun.write(state, JSON.stringify({ worker: cfg.worker.name, date: new Date().toISOString(), steps } satisfies Saved, null, 2));
      for (const line of await apply(api, local, steps, "forward")) console.log(line);
      console.log(`previous owners saved in ${state}`);
    }
  } else if (cmd === "rollback") {
    if (!existsSync(state)) throw new Error(`nothing to roll back: ${state} does not exist`);
    const saved = JSON.parse(readFileSync(state, "utf8")) as Saved;
    if (!go) console.log(render(saved.steps.map((s) => ({ ...s, to: s.from ?? "(delete)", from: s.to })), []), "\n\ndry run: add --apply to restore");
    else for (const line of await apply(api, local, saved.steps, "back")) console.log(line);
  } else {
    console.error("usage: bun tools/edge-cutover.ts plan|apply|rollback [--apply] [--json]");
    process.exit(2);
  }
}
