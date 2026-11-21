#!/usr/bin/env bun
// Scans all live HTTP endpoints across cf.toml zones, Workers, Tunnels, and R2 CDNs (+ optional lychee crawl) and auto-fixes resolvable 404s (--fix).
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  applyZoneReconciliation,
  AyCfManifestSchema,
  captureLiveSnapshot,
  createCloudflareClient,
  qualifyHostname,
  type AyCfManifest,
  type LocalCloudflareContext,
} from "../packages/cf-control-plane/src/index.ts";
import { privateDir as privateRoot } from "./local.ts";

export interface HttpTarget {
  url: string;
  zone: string;
  hostname: string;
  kind: "worker-route" | "worker-domain" | "r2-custom-domain" | "tunnel" | "direct-origin" | "workers-dev";
  tunnel?: string;
  bucket?: string;
  worker?: string;
}

export interface HttpScanRow {
  target: HttpTarget;
  status: number | string;
  ok: boolean;
  classification:
    | "ok"
    | "redirect"
    | "tunnel-catchall-404"
    | "r2-missing-root-index"
    | "worker-route-missing"
    | "origin-404"
    | "forbidden"
    | "server-error"
    | "network-error";
  detail: string;
}

const SKIP_HTTP_HOSTS = new Set(["mcp-origin.aphrody.com", "open-origin.aphrody.com", "origin-cdn.dragonballfr.com"]);
const HEALTH_PATH_BY_HOST: Record<string, string> = {
  "bot.rosegriffon.fr": "/health",
  "bot.dragonballfr.com": "/health",
  "ay-tools-release.aphrody-labs.workers.dev": "/latest.json",
  "aphrody-github.aphrody-labs.workers.dev": "/health",
  "open-source-website-assets.aphrody-labs.workers.dev": "/health",
};

export function collectHttpTargets(manifest: AyCfManifest, accountSubdomain = "aphrody-labs"): HttpTarget[] {
  const byUrl = new Map<string, HttpTarget>();
  const add = (t: HttpTarget) => {
    if (!byUrl.has(t.url)) byUrl.set(t.url, t);
  };

  for (const [zone, spec] of Object.entries(manifest.zones)) {
    const r2Hosts = new Map<string, string>();
    for (const [sub, bucket] of Object.entries(spec.r2_cnames)) {
      const hostname = qualifyHostname(sub, zone);
      r2Hosts.set(hostname.toLowerCase(), bucket);
      add({
        url: `https://${hostname}/`,
        zone,
        hostname,
        kind: "r2-custom-domain",
        bucket,
      });
    }

    for (const wd of spec.worker_domains) {
      add({
        url: `https://${wd.hostname}/`,
        zone,
        hostname: wd.hostname,
        kind: "worker-domain",
        worker: wd.service,
      });
    }

    for (const wr of spec.worker_routes) {
      const clean = wr.pattern.replace(/\*$/, "");
      const hostname = clean.split("/")[0]!;
      add({
        url: `https://${clean}`,
        zone,
        hostname,
        kind: "worker-route",
        worker: wr.script,
      });
    }

    for (const [tunnelName, subs] of Object.entries(spec.tunnel_cnames)) {
      for (const sub of subs) {
        if (/^(ssh-|mig-)/.test(sub)) continue;
        const hostname = qualifyHostname(sub, zone);
        if (SKIP_HTTP_HOSTS.has(hostname.toLowerCase())) continue;
        const path = HEALTH_PATH_BY_HOST[hostname.toLowerCase()] ?? "/";
        add({
          url: `https://${hostname}${path}`,
          zone,
          hostname,
          kind: "tunnel",
          tunnel: tunnelName,
        });
      }
    }

    for (const d of spec.direct_origins) {
      if (!d.proxied) continue;
      const hostname = qualifyHostname(d.name, zone);
      if (SKIP_HTTP_HOSTS.has(hostname.toLowerCase())) continue;
      const path = HEALTH_PATH_BY_HOST[hostname.toLowerCase()] ?? "/";
      add({
        url: `https://${hostname}${path}`,
        zone,
        hostname,
        kind: "direct-origin",
      });
    }

    for (const wsub of spec.worker_aaaa) {
      if (wsub === "*") continue;
      const hostname = qualifyHostname(wsub, zone);
      if (!r2Hosts.has(hostname.toLowerCase())) {
        add({
          url: `https://${hostname}/`,
          zone,
          hostname,
          kind: "worker-domain",
        });
      }
    }
  }

  for (const [wName, wSpec] of Object.entries(manifest.workers)) {
    if (!wSpec.workers_dev) continue;
    const hostname = `${wName}.${accountSubdomain}.workers.dev`;
    const path = HEALTH_PATH_BY_HOST[hostname.toLowerCase()] ?? "/";
    add({
      url: `https://${hostname}${path}`,
      zone: "workers.dev",
      hostname,
      kind: "workers-dev",
      worker: wName,
    });
  }

  return [...byUrl.values()].sort((a, b) => a.url.localeCompare(b.url));
}

export function classifyHttpResponse(
  target: HttpTarget,
  status: number | string,
  body: string,
  tunnelIngressHosts: Set<string>,
): Pick<HttpScanRow, "ok" | "classification" | "detail"> {
  if (typeof status !== "number") {
    return { ok: false, classification: "network-error", detail: status };
  }
  if (status >= 200 && status < 300) {
    return { ok: true, classification: "ok", detail: `HTTP ${status}` };
  }
  if (status >= 300 && status < 400) {
    return { ok: true, classification: "redirect", detail: `HTTP ${status}` };
  }
  if (status === 404) {
    if (target.kind === "r2-custom-domain") {
      return {
        ok: false,
        classification: "r2-missing-root-index",
        detail: `R2 custom domain ${target.hostname} missing root /index.html or URL rewrite`,
      };
    }
    if (target.tunnel && !tunnelIngressHosts.has(target.hostname.toLowerCase())) {
      return {
        ok: false,
        classification: "tunnel-catchall-404",
        detail: `Tunnel ${target.tunnel} lacks ingress rule for ${target.hostname} (fell through to http_status:404)`,
      };
    }
    if (target.kind === "worker-route" || target.kind === "worker-domain") {
      return {
        ok: false,
        classification: "worker-route-missing",
        detail: `Worker target ${target.url} returned 404 (${body.slice(0, 60).trim() || "empty"})`,
      };
    }
    return {
      ok: false,
      classification: "origin-404",
      detail: `Origin returned 404 (${body.slice(0, 60).trim() || "empty"})`,
    };
  }
  if (status === 403) {
    return { ok: false, classification: "forbidden", detail: `HTTP 403` };
  }
  return { ok: false, classification: "server-error", detail: `HTTP ${status}` };
}

export function buildMissingTunnelIngress(
  existingIngress: Array<{ hostname?: string; service: string; originRequest?: Record<string, unknown> }>,
  missingHosts: string[],
  defaultService = "http://127.0.0.1:8788",
): Array<{ hostname?: string; service: string; originRequest?: Record<string, unknown> }> {
  const existingSet = new Set(
    existingIngress.map((r) => String(r.hostname ?? "").toLowerCase()).filter(Boolean),
  );
  const catchAll = existingIngress.find((r) => !r.hostname) ?? { service: "http_status:404" };
  const head = existingIngress.filter((r) => Boolean(r.hostname));
  const additions = missingHosts
    .filter((h) => !existingSet.has(h.toLowerCase()))
    .sort()
    .map((hostname) => ({ hostname, service: defaultService }));
  return [...head, ...additions, catchAll];
}

async function scanTargets(targets: HttpTarget[], tunnelHostsByName: Map<string, Set<string>>): Promise<HttpScanRow[]> {
  const rows: HttpScanRow[] = [];
  let idx = 0;
  const concurrency = 16;
  async function worker() {
    while (idx < targets.length) {
      const target = targets[idx++]!;
      const ingressHosts = target.tunnel ? (tunnelHostsByName.get(target.tunnel) ?? new Set<string>()) : new Set<string>();
      try {
        const res = await fetch(target.url, { redirect: "manual", signal: AbortSignal.timeout(8000) });
        const body = res.status >= 400 ? await res.text() : "";
        if (res.status < 400) await res.body?.cancel();
        const verdict = classifyHttpResponse(target, res.status, body, ingressHosts);
        rows.push({ target, status: res.status, ...verdict });
      } catch (err) {
        const verdict = classifyHttpResponse(target, `ERR:${(err as Error).name}`, "", ingressHosts);
        rows.push({ target, status: `ERR:${(err as Error).name}`, ...verdict });
      }
    }
  }
  await Promise.all(Array.from({ length: concurrency }, worker));
  return rows.sort((a, b) => a.target.url.localeCompare(b.target.url));
}

async function ensureR2IndexAndRewrite(
  apiToken: string,
  accountId: string,
  manifest: AyCfManifest,
  zoneIdByName: Map<string, string>,
  rows: HttpScanRow[],
): Promise<string[]> {
  const client = createCloudflareClient(apiToken);
  const actions: string[] = [];
  const brokenR2 = rows.filter((r) => r.classification === "r2-missing-root-index");
  if (!brokenR2.length) return actions;

  const bucketsNeeded = new Set(brokenR2.map((r) => r.target.bucket).filter((b): b is string => Boolean(b)));
  for (const bucket of bucketsNeeded) {
    const headRes = await fetch(
      `https://api.cloudflare.com/client/v4/accounts/${accountId}/r2/buckets/${bucket}/objects/index.html`,
      { method: "GET", headers: { Authorization: `Bearer ${apiToken}` } },
    );
    if (headRes.status === 404) {
      const html = `<!DOCTYPE html><html lang="en" dir="ltr"><head><meta charset="utf-8"><title>${bucket} — Aphrody CDN</title></head><body><h1>${bucket}</h1><p>Status: OK</p></body></html>\n`;
      const putRes = await fetch(
        `https://api.cloudflare.com/client/v4/accounts/${accountId}/r2/buckets/${bucket}/objects/index.html`,
        {
          method: "PUT",
          headers: { Authorization: `Bearer ${apiToken}`, "Content-Type": "text/html; charset=utf-8" },
          body: html,
        },
      );
      if (!putRes.ok) {
        throw new Error(`Failed uploading index.html to R2 ${bucket}: HTTP ${putRes.status}`);
      }
      actions.push(`r2:${bucket}: uploaded /index.html`);
    } else {
      await headRes.body?.cancel();
    }
  }

  for (const [zoneName, spec] of Object.entries(manifest.zones)) {
    const r2Hosts = Object.keys(spec.r2_cnames).map((s) => qualifyHostname(s, zoneName));
    if (!r2Hosts.length) continue;
    const zoneId = zoneIdByName.get(zoneName);
    if (!zoneId) continue;

    let existingRules: Array<Record<string, unknown>>;
    try {
      const rs = (await client.rulesets.phases.get("http_request_transform", { zone_id: zoneId })) as unknown as {
        rules?: Array<Record<string, unknown>>;
      };
      existingRules = (rs.rules ?? []).map((r) => ({ ...r }));
    } catch {
      existingRules = [];
    }

    const hostList = r2Hosts.map((h) => `"${h}"`).join(" ");
    const wantedExpr = `(http.host in {${hostList}} and http.request.uri.path eq "/")`;
    const ref = "ay-cf-r2-root-index";
    const existing = existingRules.find((r) => r.ref === ref);
    if (!existing || existing.expression !== wantedExpr || existing.enabled !== true) {
      const nextRules = existingRules.filter((r) => r.ref !== ref).map((r) => ({
        action: r.action,
        action_parameters: r.action_parameters,
        description: r.description,
        enabled: r.enabled,
        expression: r.expression,
        ref: r.ref,
      }));
      nextRules.push({
        action: "rewrite",
        action_parameters: { uri: { path: { value: "/index.html" } } },
        description: "Rewrite R2 custom domain root / to /index.html",
        enabled: true,
        expression: wantedExpr,
        ref,
      });
      await client.rulesets.phases.update("http_request_transform", {
        zone_id: zoneId,
        rules: nextRules as never,
      });
      actions.push(`zone:${zoneName}: upserted http_request_transform rule (${r2Hosts.join(", ")})`);
    }
  }
  return actions;
}

async function runLycheeIfRequested(root: string): Promise<{ ran: boolean; summary: string }> {
  if (!Bun.which("lychee")) {
    return { ran: false, summary: "lychee binary not found in PATH" };
  }
  const proc = Bun.spawnSync(
    ["lychee", "--no-progress", "--format", "json", "README.md", "docs"],
    { cwd: root, stdout: "pipe", stderr: "pipe", windowsHide: true },
  );
  const raw = new TextDecoder().decode(proc.stdout);
  if (!raw.trim()) return { ran: true, summary: `lychee exit=${proc.exitCode}` };
  try {
    const j = JSON.parse(raw) as { total?: number; successful?: number; fails?: number };
    return {
      ran: true,
      summary: `lychee total=${j.total ?? "?"} ok=${j.successful ?? "?"} fails=${j.fails ?? 0}`,
    };
  } catch {
    return { ran: true, summary: `lychee exit=${proc.exitCode}` };
  }
}

if (import.meta.main) {
  const root = resolve(import.meta.dir, "..");
  const argv = Bun.argv.slice(2);
  const applyFix = argv.includes("--fix");
  const runLychee = argv.includes("--lychee");

  const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(join(root, "packages/cf-control-plane/cf.toml")).text()));
  const localCfgPath = join(root, "cf.local.toml");
  const localCfg = existsSync(localCfgPath)
    ? (Bun.TOML.parse(await Bun.file(localCfgPath).text()) as LocalCloudflareContext)
    : ({} as LocalCloudflareContext);
  const accountId = localCfg.cloudflare?.account_id ?? Bun.env.CLOUDFLARE_ACCOUNT_ID;
  if (!accountId) throw new Error("CLOUDFLARE_ACCOUNT_ID or cf.local.toml account_id required");

  const client = createCloudflareClient();
  const apiToken = client.apiToken ?? Bun.env.CLOUDFLARE_API_TOKEN ?? "";
  let snap = await captureLiveSnapshot(client, accountId);

  const tunnelByName = new Map(snap.tunnels.map((t) => [t.name, t]));
  const tunnelHostsByName = new Map(
    snap.tunnels.map((t) => [
      t.name,
      new Set(t.ingress.map((i) => String(i.hostname ?? "").toLowerCase()).filter(Boolean)),
    ]),
  );
  const zoneIdByName = new Map(snap.zones.map((z) => [z.name, z.id]));

  const targets = collectHttpTargets(manifest);
  let rows = await scanTargets(targets, tunnelHostsByName);
  const fixedActions: string[] = [];

  if (applyFix) {
    // 1. Reconcile any declared routes/domains/DNS drift first
    const zoneSync = await applyZoneReconciliation(client, manifest, snap, localCfg, { apply: true, prune: false });
    const routeOps =
      zoneSync.appliedDnsCreates +
      zoneSync.appliedDnsUpdates +
      zoneSync.appliedRouteCreates +
      zoneSync.appliedRouteUpdates +
      zoneSync.appliedRouteDeletes +
      zoneSync.appliedDomainCreates +
      zoneSync.appliedDomainDeletes;
    if (routeOps > 0) {
      fixedActions.push(`zone-reconcile: applied ${routeOps} DNS/route/domain update(s)`);
    }

    // 2. Fix tunnel-catchall-404 by registering declared HTTP tunnel_cnames in remote-managed tunnel config
    const tunnelMissing = new Map<string, Set<string>>();
    for (const r of rows) {
      if (r.classification === "tunnel-catchall-404" && r.target.tunnel) {
        const set = tunnelMissing.get(r.target.tunnel) ?? new Set<string>();
        set.add(r.target.hostname);
        tunnelMissing.set(r.target.tunnel, set);
      }
    }
    for (const [tunnelName, hosts] of tunnelMissing) {
      const tun = tunnelByName.get(tunnelName);
      if (!tun) continue;
      const cur = await client.zeroTrust.tunnels.cloudflared.configurations.get(tun.id, { account_id: accountId });
      const curCfg = (cur.config ?? {}) as {
        ingress?: Array<{ hostname?: string; service: string; originRequest?: Record<string, unknown> }>;
        [k: string]: unknown;
      };
      const curIngress = curCfg.ingress ?? [];
      const nextIngress = buildMissingTunnelIngress(curIngress, [...hosts]);
      if (nextIngress.length > curIngress.length) {
        await client.zeroTrust.tunnels.cloudflared.configurations.update(tun.id, {
          account_id: accountId,
          config: { ...curCfg, ingress: nextIngress } as never,
        });
        fixedActions.push(`tunnel:${tunnelName}: added ${hosts.size} missing ingress rule(s) -> http://127.0.0.1:8788`);
      }
    }

    // 3. Fix R2 Custom Domain root `/` 404 via index.html + free http_request_transform rewrite
    const r2Fixes = await ensureR2IndexAndRewrite(apiToken, accountId, manifest, zoneIdByName, rows);
    fixedActions.push(...r2Fixes);

    if (fixedActions.length > 0) {
      await Bun.sleep(2000);
      snap = await captureLiveSnapshot(client, accountId);
      const updatedTunnelHosts = new Map(
        snap.tunnels.map((t) => [
          t.name,
          new Set(t.ingress.map((i) => String(i.hostname ?? "").toLowerCase()).filter(Boolean)),
        ]),
      );
      rows = await scanTargets(targets, updatedTunnelHosts);
    }
  }

  const lycheeResult = runLychee ? await runLycheeIfRequested(root) : { ran: false, summary: "skipped" };
  const failing = rows.filter((r) => !r.ok);
  const byStatus: Record<string, number> = {};
  for (const r of rows) {
    const k = String(r.status);
    byStatus[k] = (byStatus[k] ?? 0) + 1;
  }

  const privateDir = privateRoot(root);
  mkdirSync(privateDir, { recursive: true });
  writeFileSync(
    join(privateDir, "http-scan-summary.json"),
    JSON.stringify({ scanned: rows.length, byStatus, fixedActions, lycheeResult, failing, rows }, null, 2) + "\n",
  );

  const md = [
    "# ay-cf HTTP Scan & 404 Auto-Fix Report",
    "",
    `- Endpoints scanned: **${rows.length}**`,
    `- Passing (2xx/3xx): **${rows.length - failing.length}**`,
    `- Non-2xx/3xx remaining: **${failing.length}**`,
    `- Auto-fixes applied: **${fixedActions.length}**`,
    ...(fixedActions.map((a) => `  - ${a}`)),
    ...(lycheeResult.ran ? [`- Static links (${lycheeResult.summary})`] : []),
    "",
    ...(failing.length
      ? [
          "## Remaining Findings",
          "",
          "| Status | Kind | Class | URL | Detail |",
          "| :--- | :--- | :--- | :--- | :--- |",
          ...failing.map((r) => `| ${r.status} | ${r.target.kind} | ${r.classification} | ${r.target.url} | ${r.detail} |`),
          "",
        ]
      : ["- **0 failing HTTP endpoints across all scanned zones and Workers.**"]),
  ].join("\n");
  writeFileSync(join(privateDir, "http-scan-report.md"), md + "\n");

  console.log(`HTTP scan: total=${rows.length} ok=${rows.length - failing.length} failing=${failing.length} fixed=${fixedActions.length}`);
  console.log("Status breakdown:", JSON.stringify(byStatus));
  for (const a of fixedActions) console.log("FIX:", a);
  for (const f of failing) console.log(`FAIL [${f.status}] (${f.classification}) ${f.target.url} — ${f.detail}`);
  if (runLychee) console.log(lycheeResult.summary);
  if (failing.length > 0 && !argv.includes("--no-fail")) process.exit(1);
}
