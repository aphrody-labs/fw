#!/usr/bin/env bun
// Live inventory of the Cloudflare account: Workers (routes, domains, bindings, crons, requests and non-OK of the window), R2 buckets (objects, bytes,
// operations, custom and managed domains, CORS, lifecycle), D1, queues, KV, Durable Objects, Workflows and tunnels. Read-only. It reproduces the
// snapshot of docs/cloud/platform-inventory-2026-11-20.md (ADR-16): the raw output keeps ids and goes to .private/cf/, the summary has none.
// The GraphQL helpers below are exported so that one implementation reads the analytics datasets.
//   bun tools/cf-inventory.ts [--days 7] [--out .private/cf/inventory-<date>T<hhmm>.json] [--md <file>] [--json] [--from <raw.json>]
//   --from summarizes a saved raw file instead of reading the account. Credentials: CLOUDFLARE_API_TOKEN, CLOUDFLARE_ACCOUNT_ID (or cf.local.toml).
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createCloudflareClient } from "../packages/cf-control-plane/src/index.ts";
import { loadCfLocal } from "./edge-config.ts";
import { repoRoot } from "./local.ts";

type Client = ReturnType<typeof createCloudflareClient>;
type Json = Record<string, any>;

export interface Binding {
  type: string;
  name: string;
  bucket?: string;
  script?: string;
  db?: string;
  ns?: string;
  cls?: string;
  queue?: string;
}

export interface WorkerRaw {
  bindings: Binding[];
  crons: string[];
  workers_dev: boolean;
  previews: boolean;
  deployments: Array<{ at: string; source: string }>;
  compat?: string;
  logs?: boolean;
}

export interface BucketRaw {
  created: string;
  storage_class: string;
  customDomains: unknown[];
  managed: { enabled: boolean; bucketId?: string; domain?: string };
  corsRules: number | "err";
  lifecycleRules: number | "err";
}

export interface Raw {
  at: string;
  /** Window of the analytics datasets, ISO. */
  window?: { since: string; until: string };
  workers: Record<string, WorkerRaw>;
  zones: Record<string, Array<{ pattern: string; script: string }>>;
  r2: Record<string, BucketRaw>;
  scriptsRaw: Array<Json>;
  domains: Array<{ hostname: string; service: string; zone_name: string; environment?: string }>;
  invocationsRaw: Gql;
  r2StorageRaw: Gql;
  r2OpsRaw: Gql;
  d1: Array<Json>;
  queues: Array<Json>;
  kv: Array<Json>;
  doNamespaces: Array<Json>;
  tunnels: Array<{ name: string; status: string; conns: number }>;
  workflows: Array<Json>;
}

export interface Gql {
  data?: { viewer?: { accounts?: Array<Json> } };
  errors?: unknown;
}

// ---- GraphQL analytics ---------------------------------------------------------------------------------------------

const GRAPHQL = "https://api.cloudflare.com/client/v4/graphql";

export async function graphql(token: string, query: string, variables: Json): Promise<Gql> {
  const res = await fetch(GRAPHQL, { method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" }, body: JSON.stringify({ query, variables }) });
  const body = (await res.json()) as Gql;
  if (!res.ok || body.errors) throw new Error(`graphql ${res.status}: ${JSON.stringify(body.errors ?? body).slice(0, 400)}`);
  return body;
}

const rows = (g: Gql, dataset: string): Json[] => g.data?.viewer?.accounts?.[0]?.[dataset] ?? [];

export interface Window {
  since: string;
  until: string;
}

export const windowOf = (days: number, now = new Date()): Window => ({ since: new Date(now.getTime() - days * 86_400_000).toISOString(), until: now.toISOString() });

/** Requests, errors and subrequests by script and status over the window. `byDay` adds the UTC date. */
export const invocationsQuery = (byDay: boolean): string => `query($a:String!,$s:Time!,$u:Time!){viewer{accounts(filter:{accountTag:$a}){workersInvocationsAdaptive(limit:10000,filter:{datetime_geq:$s,datetime_leq:$u}){dimensions{scriptName status${byDay ? " date" : ""}}sum{requests errors subrequests}}}}}`;

export const r2StorageQuery = `query($a:String!,$s:Time!,$u:Time!){viewer{accounts(filter:{accountTag:$a}){r2StorageAdaptiveGroups(limit:10000,orderBy:[datetime_DESC],filter:{datetime_geq:$s,datetime_leq:$u}){dimensions{bucketName datetime}max{objectCount payloadSize metadataSize}}}}}`;

/** Operations by bucket and action; `byDay` adds the UTC date. */
export const r2OpsQuery = (byDay: boolean): string => `query($a:String!,$s:Time!,$u:Time!){viewer{accounts(filter:{accountTag:$a}){r2OperationsAdaptiveGroups(limit:10000,filter:{datetime_geq:$s,datetime_leq:$u}){dimensions{actionType bucketName${byDay ? " date" : ""}}sum{requests}}}}}`;

export const fetchInvocations = (token: string, account: string, w: Window, byDay = false) => graphql(token, invocationsQuery(byDay), { a: account, s: w.since, u: w.until });
export const fetchR2Storage = (token: string, account: string, w: Window) => graphql(token, r2StorageQuery, { a: account, s: w.since, u: w.until });
export const fetchR2Ops = (token: string, account: string, w: Window, byDay = false) => graphql(token, r2OpsQuery(byDay), { a: account, s: w.since, u: w.until });

export interface ScriptTraffic {
  requests: number;
  /** Invocations whose status is not `success` (client disconnects included). */
  nonOk: number;
  byStatus: Record<string, number>;
  /** UTC date to requests, present when the query grouped by day. */
  days: Record<string, number>;
}

export function trafficByScript(g: Gql): Record<string, ScriptTraffic> {
  const out: Record<string, ScriptTraffic> = {};
  for (const r of rows(g, "workersInvocationsAdaptive")) {
    const name = String(r.dimensions.scriptName);
    const t = (out[name] ??= { requests: 0, nonOk: 0, byStatus: {}, days: {} });
    const n = Number(r.sum.requests);
    t.requests += n;
    if (r.dimensions.status !== "success") t.nonOk += n;
    t.byStatus[r.dimensions.status] = (t.byStatus[r.dimensions.status] ?? 0) + n;
    if (r.dimensions.date) t.days[r.dimensions.date] = (t.days[r.dimensions.date] ?? 0) + n;
  }
  return out;
}

export interface BucketStorage {
  objects: number;
  payloadBytes: number;
  metadataBytes: number;
  at: string;
}

/** Latest storage figure of each bucket (the query is ordered by time, newest first). */
export function storageByBucket(g: Gql): Record<string, BucketStorage> {
  const out: Record<string, BucketStorage> = {};
  for (const r of rows(g, "r2StorageAdaptiveGroups")) {
    const name = String(r.dimensions.bucketName);
    const at = String(r.dimensions.datetime);
    if (out[name] && out[name]!.at >= at) continue;
    out[name] = { objects: Number(r.max.objectCount), payloadBytes: Number(r.max.payloadSize), metadataBytes: Number(r.max.metadataSize), at };
  }
  return out;
}

/** bucket -> action -> requests (and "date:action" is not kept: use opsByDay for the per-day split). */
export function opsByBucket(g: Gql): Record<string, Record<string, number>> {
  const out: Record<string, Record<string, number>> = {};
  for (const r of rows(g, "r2OperationsAdaptiveGroups")) {
    const b = (out[String(r.dimensions.bucketName)] ??= {});
    b[r.dimensions.actionType] = (b[r.dimensions.actionType] ?? 0) + Number(r.sum.requests);
  }
  return out;
}

/** The object operations that read or write data: what "the bucket is in use" means for a deletion gate. */
export const DATA_OPS = ["GetObject", "HeadObject", "PutObject", "CopyObject", "UploadPart", "CompleteMultipartUpload", "ListObjects", "ListObjectsV2"] as const;

// ---- Account reads (SDK) ---------------------------------------------------------------------------------------------------------------------

const settle = async <T>(p: Promise<T>, fallback: T): Promise<T> => {
  try {
    return await p;
  } catch {
    return fallback;
  }
};

export function normalizeBinding(b: Json): Binding {
  const out: Binding = { type: String(b.type), name: String(b.name) };
  if (b.type === "r2_bucket") out.bucket = b.bucket_name;
  if (b.type === "service") out.script = b.service;
  if (b.type === "d1") out.db = b.id ?? b.database_id;
  if (b.type === "kv_namespace") out.ns = b.namespace_id;
  if (b.type === "queue") out.queue = b.queue_name;
  if (b.type === "durable_object_namespace") {
    out.cls = b.class_name;
    out.ns = b.namespace_id;
  }
  if (b.type === "workflow") out.cls = b.class_name;
  if (b.type === "ratelimit") out.ns = String(b.namespace_id ?? "");
  return out;
}

async function listAll<T>(page: AsyncIterable<T> | Promise<{ result?: T[] }>): Promise<T[]> {
  if (Symbol.asyncIterator in (page as object)) {
    const out: T[] = [];
    for await (const item of page as AsyncIterable<T>) out.push(item);
    return out;
  }
  return ((await page) as { result?: T[] }).result ?? [];
}

export async function captureRaw(client: Client, token: string, account: string, days: number): Promise<Raw> {
  const w = windowOf(days);
  const scripts = (await listAll(client.workers.scripts.list({ account_id: account }))) as Json[];

  const workers: Record<string, WorkerRaw> = {};
  await Promise.all(
    scripts.map(async (s) => {
      const id = String(s.id);
      const [settings, schedules, subdomain, deployments] = await Promise.all([
        settle(client.workers.scripts.settings.get(id, { account_id: account }) as Promise<Json>, {}),
        settle(client.workers.scripts.schedules.get(id, { account_id: account }) as Promise<Json>, {}),
        settle(client.workers.scripts.subdomain.get(id, { account_id: account }) as Promise<Json>, {}),
        settle(listAll(client.workers.scripts.deployments.list(id, { account_id: account }) as never) as Promise<Json[]>, []),
      ]);
      const dep = (deployments as Json[]).length ? (deployments as Json[]) : (((deployments as unknown as Json).deployments as Json[]) ?? []);
      workers[id] = {
        bindings: ((settings.bindings as Json[]) ?? []).map(normalizeBinding),
        crons: ((schedules.schedules as Json[]) ?? []).map((x) => String(x.cron)),
        workers_dev: Boolean(subdomain.enabled),
        previews: Boolean(subdomain.previews_enabled),
        deployments: dep.map((d) => ({ at: String(d.created_on), source: String(d.source ?? "") })),
        compat: s.compatibility_date,
        logs: Boolean((settings.observability as Json | undefined)?.enabled),
      };
    }),
  );

  const zoneList = (await listAll(client.zones.list({ per_page: 50 }))) as Json[];
  const zones: Raw["zones"] = {};
  const domains: Raw["domains"] = [];
  await Promise.all(
    zoneList.map(async (z) => {
      const routes = (await settle(listAll(client.workers.routes.list({ zone_id: z.id }) as never), [])) as Json[];
      zones[z.name] = routes.map((r) => ({ pattern: String(r.pattern), script: String(r.script ?? "") }));
      const ds = (await settle(listAll(client.workers.domains.list({ account_id: account, zone_id: z.id }) as never), [])) as Json[];
      for (const d of ds) domains.push({ hostname: d.hostname, service: d.service, zone_name: d.zone_name ?? z.name, environment: d.environment });
    }),
  );

  const buckets = ((await client.r2.buckets.list({ account_id: account })) as Json).buckets as Json[];
  const r2: Raw["r2"] = {};
  await Promise.all(
    buckets.map(async (b) => {
      const name = String(b.name);
      const [custom, managed, cors, lifecycle] = await Promise.all([
        settle(client.r2.buckets.domains.custom.list(name, { account_id: account }) as Promise<Json>, { domains: [] }),
        settle(client.r2.buckets.domains.managed.list(name, { account_id: account }) as Promise<Json>, { enabled: false }),
        settle(client.r2.buckets.cors.get(name, { account_id: account }) as Promise<Json>, null),
        settle(client.r2.buckets.lifecycle.get(name, { account_id: account }) as Promise<Json>, null),
      ]);
      r2[name] = {
        created: String(b.creation_date),
        storage_class: String(b.storage_class ?? "Standard"),
        customDomains: (custom.domains as unknown[]) ?? [],
        managed: { enabled: Boolean(managed.enabled), bucketId: managed.bucketId, domain: managed.domain },
        corsRules: cors ? ((cors.rules as unknown[]) ?? []).length : "err",
        lifecycleRules: lifecycle ? ((lifecycle.rules as unknown[]) ?? []).length : "err",
      };
    }),
  );

  const [invocationsRaw, r2StorageRaw, r2OpsRaw] = await Promise.all([
    fetchInvocations(token, account, w),
    fetchR2Storage(token, account, windowOf(1)),
    fetchR2Ops(token, account, w),
  ]);

  const d1 = (await settle(listAll(client.d1.database.list({ account_id: account }) as never), [])) as Json[];
  const queueList = (await settle(listAll(client.queues.list({ account_id: account }) as never), [])) as Json[];
  const queues = await Promise.all(
    queueList.map(async (q) => {
      const consumers = (await settle(listAll(client.queues.consumers.list(q.queue_id, { account_id: account }) as never), [])) as Json[];
      return { name: q.queue_name, producers: q.producers_total_count ?? (q.producers as unknown[] | undefined)?.length ?? 0, consumers: consumers.length, consumersList: consumers.map((c) => c.script ?? c.script_name ?? null) };
    }),
  );
  const kv = (await settle(listAll(client.kv.namespaces.list({ account_id: account }) as never), [])) as Json[];
  const doList = (await settle(listAll(client.durableObjects.namespaces.list({ account_id: account }) as never), [])) as Json[];
  const tunnelList = (await settle(listAll(client.zeroTrust.tunnels.cloudflared.list({ account_id: account, is_deleted: false }) as never), [])) as Json[];
  const wf = (await settle(listAll(client.workflows.list({ account_id: account }) as never), [])) as Json[];

  return {
    at: new Date().toISOString(),
    window: w,
    workers,
    zones,
    r2,
    scriptsRaw: scripts,
    domains,
    invocationsRaw,
    r2StorageRaw,
    r2OpsRaw,
    d1: d1.map((d) => ({ name: d.name, id: d.uuid ?? d.id, size: d.file_size ?? 0, created: d.created_at })),
    queues,
    kv: kv.map((k) => ({ title: k.title, id: k.id })),
    doNamespaces: doList.map((d) => ({ name: d.name, script: d.script, class: d.class, id: d.id })),
    tunnels: tunnelList.map((t) => ({ name: String(t.name), status: String(t.status), conns: Array.isArray(t.connections) ? t.connections.length : 0 })),
    workflows: wf.map((x) => ({ name: x.name, script: x.script_name, class: x.class_name })),
  };
}

// ---- Summary (no ids) ------------------------------------------------------------------------------------------------------------------------

export interface WorkerSummary {
  name: string;
  requests: number;
  nonOk: number;
  routes: string[];
  domains: string[];
  workersDev: boolean;
  crons: string[];
  buckets: string[];
  services: string[];
  durableObjects: string[];
  lastDeploy: string;
}

export interface BucketSummary {
  name: string;
  objects: number;
  bytes: number;
  created: string;
  customDomains: number;
  managedEnabled: boolean;
  corsRules: number | "err";
  lifecycleRules: number | "err";
  ops: Record<string, number>;
  bindingWorkers: string[];
}

export interface Summary {
  at: string;
  windowDays: number;
  workers: WorkerSummary[];
  buckets: BucketSummary[];
  totals: { workers: number; requests: number; nonOk: number; unattributed: number; routes: number; domains: number; buckets: number; objects: number; bytes: number };
  d1: string[];
  queues: Array<{ name: string; producers: number; consumers: number }>;
  kvNamespaces: number;
  durableObjectNamespaces: string[];
  workflows: string[];
  tunnels: Array<{ name: string; status: string; connections: number }>;
}

export function summarize(raw: Raw, days = 7): Summary {
  const traffic = trafficByScript(raw.invocationsRaw);
  const storage = storageByBucket(raw.r2StorageRaw);
  const ops = opsByBucket(raw.r2OpsRaw);
  const routesOf = (script: string) =>
    Object.values(raw.zones).flatMap((rs) => rs.filter((r) => r.script === script).map((r) => r.pattern));
  const names = [...new Set([...Object.keys(raw.workers), ...raw.scriptsRaw.map((s) => String(s.id))])];

  const workers: WorkerSummary[] = names
    .map((name) => {
      const w = raw.workers[name];
      const t = traffic[name];
      const sc = raw.scriptsRaw.find((s) => s.id === name);
      const deploy = [...(w?.deployments ?? []).map((d) => d.at), sc?.modified_on].filter(Boolean).sort().at(-1);
      return {
        name,
        requests: t?.requests ?? 0,
        nonOk: t?.nonOk ?? 0,
        routes: routesOf(name),
        domains: raw.domains.filter((d) => d.service === name).map((d) => d.hostname),
        workersDev: w?.workers_dev ?? false,
        crons: w?.crons ?? [],
        buckets: [...new Set((w?.bindings ?? []).filter((b) => b.type === "r2_bucket").map((b) => String(b.bucket)))].sort(),
        services: [...new Set((w?.bindings ?? []).filter((b) => b.type === "service").map((b) => String(b.script)))].sort(),
        durableObjects: (w?.bindings ?? []).filter((b) => b.type === "durable_object_namespace").map((b) => String(b.cls)),
        lastDeploy: String(deploy ?? "").slice(0, 10),
      };
    })
    .sort((a, b) => b.requests - a.requests || a.name.localeCompare(b.name));

  const buckets: BucketSummary[] = Object.entries(raw.r2)
    .map(([name, b]) => ({
      name,
      objects: storage[name]?.objects ?? 0,
      bytes: storage[name]?.payloadBytes ?? 0,
      created: b.created.slice(0, 10),
      customDomains: b.customDomains.length,
      managedEnabled: b.managed.enabled,
      corsRules: b.corsRules,
      lifecycleRules: b.lifecycleRules,
      ops: ops[name] ?? {},
      bindingWorkers: workers.filter((w) => w.buckets.includes(name)).map((w) => w.name).sort(),
    }))
    .sort((a, b) => b.bytes - a.bytes);

  const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
  return {
    at: raw.at,
    windowDays: days,
    workers,
    buckets,
    totals: {
      workers: workers.length,
      requests: sum(Object.values(traffic).map((t) => t.requests)),
      nonOk: sum(Object.values(traffic).map((t) => t.nonOk)),
      unattributed: sum(Object.entries(traffic).filter(([k]) => !names.includes(k)).map(([, t]) => t.requests)),
      routes: sum(Object.values(raw.zones).map((r) => r.length)),
      domains: raw.domains.length,
      buckets: buckets.length,
      objects: sum(buckets.map((b) => b.objects)),
      bytes: sum(buckets.map((b) => b.bytes)),
    },
    d1: raw.d1.map((d) => String(d.name)),
    queues: raw.queues.map((q) => ({ name: String(q.name), producers: Number(q.producers), consumers: Number(q.consumers) })),
    kvNamespaces: raw.kv.length,
    durableObjectNamespaces: raw.doNamespaces.map((d) => String(d.name)),
    workflows: raw.workflows.map((x) => String(x.name)),
    tunnels: raw.tunnels.map((t) => ({ name: t.name, status: t.status, connections: t.conns })),
  };
}

const n = (x: number): string => x.toLocaleString("en-US");
const gb = (b: number): string => (b / 1e9).toFixed(2);

export function renderMarkdown(s: Summary): string {
  const w = s.workers.map(
    (x) =>
      `| ${x.name} | ${n(x.requests)} | ${n(x.nonOk)} | ${x.routes.length} | ${x.domains.join(", ") || "none"} | ${x.workersDev ? "yes" : "no"} | ${x.crons.join("; ") || "none"} | ${[...x.buckets.map((b) => `R2 ${b}`), ...x.services.map((v) => `service ${v}`), ...x.durableObjects.map((d) => `DO ${d}`)].join("; ") || "none"} | ${x.lastDeploy} |`,
  );
  const b = s.buckets.map((x) => {
    const o = (k: string) => n(x.ops[k] ?? 0);
    return `| ${x.name} | ${n(x.objects)} | ${gb(x.bytes)} | ${x.created} | ${o("GetObject")} | ${o("HeadObject")} | ${o("PutObject")} | ${x.bindingWorkers.join(", ") || "none"} |`;
  });
  return [
    `# Cloudflare inventory, ${s.at.slice(0, 10)}`,
    "",
    `Read by \`tools/cf-inventory.ts\` (window of ${s.windowDays} days ending ${s.at}). ${s.totals.workers} Worker scripts, ${n(s.totals.requests)} requests (${n(s.totals.nonOk)} not OK, ${n(s.totals.unattributed)} without a script name), ${s.totals.routes} zone routes, ${s.totals.domains} custom domains; ${s.totals.buckets} R2 buckets, ${n(s.totals.objects)} objects, ${gb(s.totals.bytes)} GB.`,
    "",
    "| Worker | Req | non-OK | Routes | Custom domains | workers.dev | Cron | Bindings | Last deploy |",
    "|---|---:|---:|---:|---|---|---|---|---|",
    ...w,
    "",
    "| Bucket | Objects | GB | Created | GetObject | HeadObject | PutObject | Binding Workers |",
    "|---|---:|---:|---|---:|---:|---:|---|",
    ...b,
    "",
    `D1: ${s.d1.join(", ") || "none"}; queues: ${s.queues.map((q) => `${q.name} (${q.producers} producers, ${q.consumers} consumers)`).join(", ") || "none"}; KV namespaces: ${s.kvNamespaces}; Durable Object namespaces: ${s.durableObjectNamespaces.length}; Workflows: ${s.workflows.length}; tunnels: ${s.tunnels.map((t) => `${t.name} ${t.status} ${t.connections}`).join(", ") || "none"}.`,
    "",
  ].join("\n");
}

// ---- CLI -------------------------------------------------------------------------------------------------------------------------------------

export function credentials(): { token: string; account: string } {
  const token = Bun.env.CLOUDFLARE_API_TOKEN;
  const account = Bun.env.CLOUDFLARE_ACCOUNT_ID ?? loadCfLocal().cloudflare?.account_id;
  if (!token) throw new Error("CLOUDFLARE_API_TOKEN is required");
  if (!account) throw new Error("account id: set CLOUDFLARE_ACCOUNT_ID or cf.local.toml [cloudflare].account_id");
  return { token, account };
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const opt = (k: string): string | undefined => {
    const i = args.indexOf(k);
    return i < 0 ? undefined : args[i + 1];
  };
  const days = Number(opt("--days") ?? 7);
  const from = opt("--from");
  let raw: Raw;
  if (from) raw = JSON.parse(readFileSync(from, "utf8")) as Raw;
  else {
    const { token, account } = credentials();
    raw = await captureRaw(createCloudflareClient(token), token, account, days);
    const out = opt("--out") ?? join(repoRoot, ".private", "cf", `inventory-${raw.at.slice(0, 16).replaceAll(":", "")}.json`);
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out, JSON.stringify(raw, null, 1) + "\n");
    console.error(`raw inventory (with ids) written to ${out}`);
  }
  const summary = summarize(raw, days);
  const md = opt("--md");
  if (md) writeFileSync(md, renderMarkdown(summary));
  console.log(args.includes("--json") ? JSON.stringify(summary, null, 2) : renderMarkdown(summary));
}
