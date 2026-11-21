import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { type Gql, type Raw, normalizeBinding, opsByBucket, renderMarkdown, storageByBucket, summarize, trafficByScript, windowOf } from "./cf-inventory.ts";
import { repoRoot } from "./local.ts";

const gql = (dataset: string, groups: unknown[]): Gql => ({ data: { viewer: { accounts: [{ [dataset]: groups }] } } });

describe("analytics parsing", () => {
  test("traffic counts every status, non-OK is everything but success, days are kept", () => {
    const g = gql("workersInvocationsAdaptive", [
      { dimensions: { scriptName: "a", status: "success", date: "2026-11-19" }, sum: { requests: 90 } },
      { dimensions: { scriptName: "a", status: "scriptThrewException", date: "2026-11-19" }, sum: { requests: 10 } },
      { dimensions: { scriptName: "a", status: "success", date: "2026-11-20" }, sum: { requests: 5 } },
      { dimensions: { scriptName: "__unknown__", status: "success" }, sum: { requests: 7 } },
    ]);
    const t = trafficByScript(g);
    expect(t.a).toEqual({ requests: 105, nonOk: 10, byStatus: { success: 95, scriptThrewException: 10 }, days: { "2026-11-19": 100, "2026-11-20": 5 } });
    expect(t.__unknown__!.requests).toBe(7);
  });

  test("storage keeps the newest figure of a bucket", () => {
    const g = gql("r2StorageAdaptiveGroups", [
      { dimensions: { bucketName: "b", datetime: "2026-11-20T10:00:00Z" }, max: { objectCount: 1, payloadSize: 10, metadataSize: 1 } },
      { dimensions: { bucketName: "b", datetime: "2026-11-20T18:00:00Z" }, max: { objectCount: 3, payloadSize: 30, metadataSize: 2 } },
      { dimensions: { bucketName: "b", datetime: "2026-11-20T12:00:00Z" }, max: { objectCount: 2, payloadSize: 20, metadataSize: 1 } },
    ]);
    expect(storageByBucket(g).b).toEqual({ objects: 3, payloadBytes: 30, metadataBytes: 2, at: "2026-11-20T18:00:00Z" });
  });

  test("operations add up by bucket and action", () => {
    const g = gql("r2OperationsAdaptiveGroups", [
      { dimensions: { bucketName: "b", actionType: "GetObject", date: "2026-11-19" }, sum: { requests: 4 } },
      { dimensions: { bucketName: "b", actionType: "GetObject", date: "2026-11-20" }, sum: { requests: 6 } },
      { dimensions: { bucketName: "b", actionType: "PutObject" }, sum: { requests: 1 } },
    ]);
    expect(opsByBucket(g)).toEqual({ b: { GetObject: 10, PutObject: 1 } });
  });

  test("a window ends now and spans the days asked", () => {
    const w = windowOf(7, new Date("2026-11-20T19:00:00Z"));
    expect(w).toEqual({ since: "2026-11-13T19:00:00.000Z", until: "2026-11-20T19:00:00.000Z" });
  });

  test("bindings keep what a reader needs and no secret value", () => {
    expect(normalizeBinding({ type: "r2_bucket", name: "ASSETS", bucket_name: "aphrody-assets" })).toEqual({ type: "r2_bucket", name: "ASSETS", bucket: "aphrody-assets" });
    expect(normalizeBinding({ type: "service", name: "S", service: "auto" }).script).toBe("auto");
    expect(normalizeBinding({ type: "secret_text", name: "K", text: "hunter2" })).toEqual({ type: "secret_text", name: "K" });
  });
});

const small: Raw = {
  at: "2026-11-20T19:00:00.000Z",
  workers: { a: { bindings: [{ type: "r2_bucket", name: "X", bucket: "b" }], crons: [], workers_dev: true, previews: false, deployments: [{ at: "2026-11-18T10:00:00Z", source: "api" }] } },
  zones: { "z.example": [{ pattern: "z.example/*", script: "a" }] },
  r2: { b: { created: "2026-11-01T00:00:00Z", storage_class: "Standard", customDomains: [], managed: { enabled: false }, corsRules: 0, lifecycleRules: 1 } },
  scriptsRaw: [{ id: "a", modified_on: "2026-11-19T00:00:00Z" }],
  domains: [{ hostname: "h.z.example", service: "a", zone_name: "z.example" }],
  invocationsRaw: gql("workersInvocationsAdaptive", [
    { dimensions: { scriptName: "a", status: "success" }, sum: { requests: 8 } },
    { dimensions: { scriptName: "__unknown__", status: "success" }, sum: { requests: 2 } },
  ]),
  r2StorageRaw: gql("r2StorageAdaptiveGroups", [{ dimensions: { bucketName: "b", datetime: "2026-11-20T18:00:00Z" }, max: { objectCount: 3, payloadSize: 3e9, metadataSize: 0 } }]),
  r2OpsRaw: gql("r2OperationsAdaptiveGroups", [{ dimensions: { bucketName: "b", actionType: "GetObject" }, sum: { requests: 9 } }]),
  d1: [],
  queues: [],
  kv: [],
  doNamespaces: [],
  tunnels: [],
  workflows: [],
};

describe("summary", () => {
  test("totals, unattributed traffic and the markdown carry the figures", () => {
    const s = summarize(small);
    expect(s.totals).toMatchObject({ workers: 1, requests: 10, unattributed: 2, routes: 1, domains: 1, buckets: 1, objects: 3, bytes: 3e9 });
    expect(s.workers[0]).toMatchObject({ name: "a", requests: 8, routes: ["z.example/*"], domains: ["h.z.example"], buckets: ["b"], lastDeploy: "2026-11-19" });
    const md = renderMarkdown(s);
    expect(md).toContain("| a | 8 | 0 | 1 |");
    expect(md).toContain("| b | 3 | 3.00 | 2026-11-01 | 9 | 0 | 0 | a |");
  });

  test("the summary holds no id", () => {
    const text = JSON.stringify(summarize(small));
    expect(text).not.toMatch(/[0-9a-f]{32}/);
  });
});

// The reproduction gate of ADR-16 WP1: the saved snapshot of 2026-11-20 (git-ignored, with ids) summarizes to the figures of the inventory document.
const saved = join(repoRoot, ".private", "cf", "inventory-2026-11-20.json");
describe.skipIf(!existsSync(saved))("snapshot of 2026-11-20", () => {
  test("25 Workers, 12 buckets and the figures of the inventory", () => {
    const s = summarize(JSON.parse(readFileSync(saved, "utf8")) as Raw);
    expect(s.totals).toMatchObject({ workers: 25, buckets: 12, routes: 59, domains: 26, requests: 954029, objects: 265805 });
    const dist = s.buckets.find((b) => b.name === "aphrody-dist")!;
    expect([dist.objects, dist.bytes]).toEqual([40986, 18328252275]);
    const github = s.workers.find((w) => w.name === "aphrody-github")!;
    expect([github.requests, github.nonOk]).toEqual([259331, 104472]);
    expect(s.workers.find((w) => w.name === "ay-edge")!.routes).toHaveLength(14);
  });
});
