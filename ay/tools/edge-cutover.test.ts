import { describe, expect, test } from "bun:test";
import { applySteps, mergeSaved, missingScripts, plan, render, type Api, type Live } from "./edge-cutover.ts";
import { loadEdgeConfig, routeRows } from "./edge-config.ts";

const cfg = loadEdgeConfig();
const rows = routeRows(cfg);
const root = cfg.domains.root!;
const name = cfg.worker.name;

const live: Live = {
  routes: {
    [root]: [
      { id: "r1", pattern: `releases.${root}/*`, script: "aphrody-worker" },
      { id: "r2", pattern: `docs.${root}/*`, script: name },
      { id: "r3", pattern: `${root}/github`, script: "aphrody-worker" },
    ],
    [cfg.domains.shenron!]: [{ id: "r9", pattern: `cdn.${cfg.domains.shenron}/*`, script: "shenron-cdn" }],
  },
  domains: [{ hostname: `auto.${root}`, service: "auto" }],
  scripts: ["aphrody-worker", "aphrody-learn", "auto", "shenron-cdn"],
  unproxied: [`wasm.${root}`],
};

describe("edge cutover plan", () => {
  const steps = plan(cfg, rows, live);
  const by = (p: string) => steps.find((s) => s.pattern === p)!;

  test("a route owned by another script is re-pointed, one already on the Worker is kept, a missing one is created", () => {
    expect(by(`releases.${root}/*`)).toMatchObject({ action: "repoint", from: "aphrody-worker", to: name, routeId: "r1" });
    expect(by(`docs.${root}/*`).action).toBe("keep");
    expect(by(`wasm.${root}/*`)).toMatchObject({ action: "create", from: null });
  });

  test("the custom domain is a domain step, not a route", () => {
    expect(by(`auto.${root}`)).toMatchObject({ kind: "domain", action: "repoint", from: "auto" });
  });

  test("routes are grouped by the zone of their host", () => {
    expect(by(`cdn.${cfg.domains.shenron}/*`).zone).toBe(cfg.domains.shenron);
    expect(by(`releases.${root}/*`).zone).toBe(root);
  });

  test("scripts the Worker binds to are checked against the account", () => {
    expect(missingScripts(cfg, live)).toEqual([]);
    expect(missingScripts(cfg, { ...live, scripts: ["auto"] })).toEqual(["aphrody-learn", "aphrody-worker", "shenron-cdn"]);
    expect(render(steps, ["x"], live.unproxied)).toContain("missing service scripts");
    expect(render(steps, [], live.unproxied)).toContain(`no proxied DNS record, add one before the route can serve: wasm.${root}`);
  });
});

describe("edge cutover apply and rollback", () => {
  function recorder() {
    const calls: Array<{ path: string; method: string; body?: unknown }> = [];
    const api: Api = async (path, init) => {
      calls.push({ path, method: init?.method ?? "GET", body: init?.body });
      return { success: true, result: { id: "new" } };
    };
    return { calls, api };
  }
  const local = { accountId: "acc", zones: { [cfg.domains.root!]: "zone-root", [cfg.domains.shenron!]: "zone-shenron" } };

  test("forward writes only what changes and rollback restores the previous owners", async () => {
    const { calls, api } = recorder();
    const steps = plan(cfg, rows, live);
    await applySteps(api, local, steps, "forward");
    expect(calls.some((c) => c.method === "PUT" && c.path === "zones/zone-root/workers/routes/r1" && (c.body as { script: string }).script === name)).toBe(true);
    expect(calls.some((c) => c.path.endsWith("/r2"))).toBe(false);
    expect(calls.some((c) => c.method === "POST" && (c.body as { pattern: string }).pattern === `wasm.${root}/*`)).toBe(true);
    expect(calls.some((c) => c.path === "accounts/acc/workers/domains" && (c.body as { service: string }).service === name)).toBe(true);
    calls.length = 0;
    await applySteps(api, local, steps, "back");
    expect(calls.some((c) => c.method === "PUT" && c.path.endsWith("/r1") && (c.body as { script: string }).script === "aphrody-worker")).toBe(true);
    expect(calls.some((c) => c.method === "DELETE" && c.path.endsWith("/new"))).toBe(true);
    expect(calls.some((c) => c.path === "accounts/acc/workers/domains" && (c.body as { service: string }).service === "auto")).toBe(true);
  });

  test("an API failure stops the run with the endpoint and the error", async () => {
    const api: Api = async () => ({ success: false, errors: [{ code: 10020 }] });
    await expect(applySteps(api, local, plan(cfg, rows, live), "forward")).rejects.toThrow(/PUT zones\/zone-root\/workers\/routes\/r1.*10020/);
  });
});

describe("resumed apply", () => {
  test("a step an earlier run already moved keeps its original owner in the saved state", () => {
    const first = plan(cfg, rows, live);
    const after: Live = { ...live, routes: { ...live.routes, [root]: live.routes[root]!.map((r) => ({ ...r, script: name })) } };
    const second = plan(cfg, rows, after);
    const merged = mergeSaved(second, { worker: name, date: "", steps: first });
    expect(merged.find((s) => s.pattern === `releases.${root}/*`)).toMatchObject({ action: "repoint", from: "aphrody-worker" });
  });
});
