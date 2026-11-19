import { describe, expect, test } from "bun:test";
import { loadEdgeConfig, renderWrangler, routeRows, serviceBindings, zoneOf } from "../../../tools/edge-config.ts";
import { matchRoute, resolve } from "../src/router.ts";

const cfg = loadEdgeConfig();
const rows = routeRows(cfg);
const infos = Object.fromEntries(Object.keys(cfg.services).map((n) => [n, { binding: serviceBindings(cfg)[n] ?? null }]));
const root = cfg.domains.root!;

describe("route table", () => {
  test("rows come from edge.toml with the domains of ay.toml", () => {
    expect(rows.map((r) => r.pattern)).toContain(`releases.${root}/*`);
    expect(rows.every((r) => !r.pattern.includes("{"))).toBe(true);
  });

  test("the longest path wins on a shared host", () => {
    expect(matchRoute(rows, root, "/download")?.service).toBe("releases");
    expect(matchRoute(rows, root, "/download/x")?.service).toBe("releases");
    expect(matchRoute(rows, root, "/github")?.service).toBe("aphrody");
    expect(matchRoute(rows, root, "/r2-assets/v1/a")?.service).toBe("assets");
    expect(matchRoute(rows, root, "/pricing")).toBeUndefined();
  });

  test("hosts select services, case-insensitively", () => {
    expect(matchRoute(rows, `RELEASES.${root}`, "/manifest.json")?.service).toBe("releases");
    expect(matchRoute(rows, `docs.${root}`, "/x")?.service).toBe("aphrody");
    expect(matchRoute(rows, `learn.${root}`, "/")?.service).toBe("learn");
    expect(matchRoute(rows, `cdn.${cfg.domains.shenron}`, "/a.png")?.service).toBe("shenron");
    expect(matchRoute(rows, `auto.${root}`, "/api")?.service).toBe("auto");
  });

  test("an unknown host resolves only through the preview prefix", () => {
    expect(resolve(rows, infos, "ay-edge.workers.dev", "/manifest.json")).toBeUndefined();
    expect(resolve(rows, infos, "localhost", "/_/releases/manifest.json")).toEqual({ service: "releases", path: "/manifest.json", preview: true });
    expect(resolve(rows, infos, "localhost", "/_/releases")).toEqual({ service: "releases", path: "/", preview: true });
    expect(resolve(rows, infos, "localhost", "/_/nope/x")).toBeUndefined();
  });

  test("a known host never takes the preview prefix", () => {
    expect(resolve(rows, infos, `docs.${root}`, "/_/releases/x")?.service).toBe("aphrody");
  });
});

describe("wrangler render", () => {
  test("every route has a zone and the services list matches the bindings", () => {
    const w = renderWrangler(cfg) as { routes: Array<{ pattern: string; zone_name?: string; custom_domain?: boolean }>; services: Array<{ binding: string }> };
    expect(w.routes.length).toBe(rows.length);
    for (const r of w.routes) expect(r.custom_domain || zoneOf(r.pattern.split("/")[0]!, cfg.domains)).toBeTruthy();
    expect(w.services.map((s) => s.binding).sort()).toEqual(Object.values(serviceBindings(cfg)).sort());
  });

  test("the no-routes variant uploads the script without touching a zone", () => {
    const w = renderWrangler(cfg, { routes: false }) as Record<string, unknown>;
    expect(w.routes).toBeUndefined();
    expect(w.workers_dev).toBe(true);
    expect((renderWrangler(cfg) as Record<string, unknown>).workers_dev).toBe(false);
    expect(w.vars).toBeDefined();
  });

  test("a disabled service has neither route nor binding", () => {
    expect(rows.some((r) => r.service === "fw")).toBe(false);
    expect(Object.values(serviceBindings(cfg))).not.toContain("SVC_FW");
  });
});
