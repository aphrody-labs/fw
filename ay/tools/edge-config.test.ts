import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { checkEdge, edgeDir, loadCfLocal, loadCfManifest, loadEdgeConfig, renderText, routeRows } from "./edge-config.ts";

const cfg = loadEdgeConfig();
const wrangler = readFileSync(join(edgeDir, "wrangler.jsonc"), "utf8").replaceAll("\r\n", "\n");
const local = loadCfLocal();
const fresh = () => structuredClone(loadCfManifest());
const messages = (manifest = fresh(), config = cfg, text = wrangler) => checkEdge(config, manifest, text, local).map((p) => `${p.where}: ${p.message}`);

describe("cf.toml, edge.toml and wrangler.jsonc", () => {
  test("agree today", () => {
    expect(messages()).toEqual([]);
    expect(wrangler).toBe(renderText(cfg));
  });

  test("every edge route is declared in cf.toml as the edge Worker's, every other route is external", () => {
    const manifest = fresh();
    const worker = cfg.worker.name;
    const owned = new Set<string>();
    for (const spec of Object.values(manifest.zones)) {
      for (const r of spec.worker_routes) if (r.script === worker) owned.add(r.pattern);
      for (const d of spec.worker_domains) if (d.service === worker) owned.add(`${d.hostname}/*`);
    }
    expect([...owned].sort()).toEqual(routeRows(cfg).map((r) => r.pattern).sort());
  });

  test("a route owned by the Worker and by an external script is reported", () => {
    const manifest = fresh();
    const row = routeRows(cfg).find((r) => !r.custom)!;
    for (const spec of Object.values(manifest.zones)) for (const r of spec.worker_routes) if (r.pattern === row.pattern) r.script = "aphrody-webos";
    expect(messages(manifest).join("\n")).toContain("owned twice");
  });

  test("a cf.toml route whose script is neither the Worker nor external is reported", () => {
    const manifest = fresh();
    manifest.zones["aphrody.com"]!.worker_routes.push({ pattern: "ghost.aphrody.com/*", script: "ghost" });
    expect(messages(manifest).join("\n")).toContain("neither the edge Worker");
  });

  test("an edge route missing from cf.toml, or without proxied DNS, is reported", () => {
    const manifest = fresh();
    manifest.zones["aphrody.com"]!.worker_routes = manifest.zones["aphrody.com"]!.worker_routes.filter((r) => r.pattern !== "wasm.aphrody.com/*");
    manifest.zones["aphrody.com"]!.worker_aaaa = manifest.zones["aphrody.com"]!.worker_aaaa.filter((h) => h !== "assets");
    const out = messages(manifest).join("\n");
    expect(out).toContain("wasm.aphrody.com/*: not declared in cf.toml");
    expect(out).toContain("assets.aphrody.com has no proxied DNS record");
  });

  test("a hint on a route the service does not own, and a stale wrangler.jsonc, are reported", () => {
    const config = structuredClone(cfg);
    config.services.assets!.hints = { "nowhere.{root}/*": { cache: { ttl: 1 } } };
    expect(messages(fresh(), config).join("\n")).toContain("is not a route of the service");
    expect(messages(fresh(), cfg, wrangler.replace('"service": "wasm"', '"service": "assets"')).join("\n")).toContain("vars differs");
  });

  test("the Worker contract of cf.toml matches edge.toml", () => {
    const manifest = fresh();
    manifest.workers["ay-edge"]!.cpu_ms = 500;
    expect(messages(manifest).join("\n")).toContain("cpu_ms 500 differs");
  });
});
