#!/usr/bin/env bun
// Probes every public host of cf.toml and apps/edge/edge.toml over HTTP/3 and HTTP/2 and records the negotiated protocol and Alt-Svc.
//   bun tools/cf-h3-probe.ts [--host <name>...]   writes .private/h3-evidence.json (git-ignored); exit 1 when a resolvable proxied host is not h3
// Verdicts: h3 (QUIC answered and Alt-Svc advertises it), no-dns (declared but absent from live DNS), r2-custom-domain (served by R2 directly,
// which answers HTTP/2 without Alt-Svc whatever the zone setting is: front it with a Worker route to get h3), fail (the only verdict that exits 1).
// The HTTP/3 request is a real QUIC connection from Bun's own client (`protocol: "http3"` rejects instead of falling back to TCP); the HTTP/2
// request is the default fetch and gives the Alt-Svc header a browser would act on. Hosts without the Cloudflare proxy are reported, not failed.
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { AyCfManifestSchema, publicHosts, type LocalCloudflareContext, type PublicHost } from "../packages/cf-control-plane/src/index.ts";
import { loadEdgeConfig, routeRows } from "./edge-config.ts";
import { privatePath, repoRoot } from "./local.ts";

export interface HostProbe {
  host: string;
  zone: string;
  proxied: boolean;
  via: string[];
  verdict: "h3" | "no-dns" | "r2-custom-domain" | "fail";
  h3: { ok: boolean; status?: number; ms?: number; error?: string };
  h2: { ok: boolean; status?: number; protocol?: string; altSvc?: string | null; ms?: number; error?: string };
}

const TIMEOUT_MS = 15_000;

async function request(host: string, protocol: "http3" | undefined): Promise<{ status: number; altSvc: string | null; ms: number }> {
  const start = performance.now();
  const res = await fetch(`https://${host}/`, {
    method: "HEAD",
    redirect: "manual",
    protocol,
    signal: AbortSignal.timeout(TIMEOUT_MS),
  } as RequestInit);
  await res.body?.cancel();
  return { status: res.status, altSvc: res.headers.get("alt-svc"), ms: Math.round(performance.now() - start) };
}

export async function probeHost(h: Pick<PublicHost, "host" | "zone" | "proxied" | "via">, r2Domains: ReadonlySet<string> = new Set()): Promise<HostProbe> {
  const out: HostProbe = { ...h, verdict: "fail", h3: { ok: false }, h2: { ok: false } };
  try {
    const r = await request(h.host, "http3");
    out.h3 = { ok: true, status: r.status, ms: r.ms };
  } catch (e) {
    out.h3 = { ok: false, error: String((e as Error).message ?? e).slice(0, 160) };
  }
  try {
    const r = await request(h.host, undefined);
    out.h2 = { ok: true, status: r.status, altSvc: r.altSvc, ms: r.ms };
  } catch (e) {
    out.h2 = { ok: false, error: String((e as Error).message ?? e).slice(0, 160) };
  }
  const dnsMissing = /DNSResolutionFailed|ENOTFOUND/.test(`${out.h3.error ?? ""} ${out.h2.error ?? ""}`);
  out.verdict = out.h3.ok && out.h2.altSvc?.includes("h3") ? "h3" : dnsMissing ? "no-dns" : r2Domains.has(h.host) ? "r2-custom-domain" : "fail";
  return out;
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(join(repoRoot, "packages/cf-control-plane/cf.toml")).text()));
  const localFile = Bun.file(join(repoRoot, "cf.local.toml"));
  const local = (await localFile.exists() ? Bun.TOML.parse(await localFile.text()) : {}) as LocalCloudflareContext;
  const r2Domains = new Set(Object.values(manifest.buckets).flatMap((b) => b.custom_domains.map((d) => d.toLowerCase())));
  const hosts = new Map(publicHosts(manifest, local).filter((h) => !h.host.startsWith("*")).map((h) => [h.host, h]));
  const edge = loadEdgeConfig();
  for (const row of routeRows(edge)) {
    if (!hosts.has(row.host)) hosts.set(row.host, { host: row.host, zone: "", proxied: true, via: ["edge"] });
  }
  const only = args.flatMap((a, i) => (args[i - 1] === "--host" ? [a] : []));
  const list = [...hosts.values()].filter((h) => (only.length ? only.includes(h.host) : h.proxied));
  const results: HostProbe[] = [];
  const queue = [...list];
  await Promise.all(
    Array.from({ length: 8 }, async () => {
      for (let h = queue.shift(); h; h = queue.shift()) results.push(await probeHost(h, r2Domains));
    }),
  );
  results.sort((a, b) => a.host.localeCompare(b.host));
  const report = {
    date: new Date().toISOString(),
    client: `bun ${Bun.version} fetch protocol=http3`,
    hosts: results,
    summary: {
      probed: results.length,
      h3: results.filter((r) => r.verdict === "h3").length,
      noDns: results.filter((r) => r.verdict === "no-dns").map((r) => r.host),
      r2CustomDomain: results.filter((r) => r.verdict === "r2-custom-domain").map((r) => r.host),
      fail: results.filter((r) => r.verdict === "fail").map((r) => r.host),
      dnsOnlySkipped: [...hosts.values()].filter((h) => !h.proxied).length,
    },
  };
  writeFileSync(privatePath("h3-evidence.json", repoRoot, true), JSON.stringify(report, null, 2) + "\n");
  for (const r of results) {
    console.log(`${r.verdict.padEnd(17)}${r.host.padEnd(34)} h3=${r.h3.ok ? `ok ${r.h3.status} ${r.h3.ms}ms` : `FAIL ${r.h3.error}`}  alt-svc=${r.h2.altSvc ?? (r.h2.ok ? "none" : `h2 FAIL ${r.h2.error}`)}`);
  }
  console.log(JSON.stringify(report.summary));
  process.exit(report.summary.fail.length ? 1 : 0);
}
