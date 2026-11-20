import { describe, expect, test } from "bun:test";
import {
  DEFAULT_QUOTA_POLICY,
  evaluateQuotaRouting,
  type QuotaTelemetry,
} from "./cf-quota.ts";

const GIB = 1024 ** 3;

function baseTelemetry(overrides: Partial<QuotaTelemetry> = {}): QuotaTelemetry {
  return {
    cloudflare: {
      tier: "free",
      workersCount: 8,
      dailyWorkerRequests: 12_000,
      dailyWorkerErrors: 5,
      r2StorageBytes: 2 * GIB,
      r2ClassAMonthly: 50_000,
      r2ClassBMonthly: 200_000,
      kvNamespaces: 2,
      kvDailyReads: 1_000,
      kvDailyWrites: 10,
      d1Databases: 1,
      d1StorageBytes: 100 * 1024 * 1024,
      d1DailyRowsRead: 5_000,
      d1DailyRowsWritten: 100,
    },
    githubActions: {
      monthlyMinutesUsed: 300,
      repos: [
        { repo: "aphrody-labs/ay", cacheBytes: 1 * GIB },
        { repo: "aphrody-labs/aphrody", cacheBytes: 2 * GIB },
      ],
    },
    vpsFleet: [
      { role: "prod", reachable: true, diskUsedPct: 40, load1m: 1.2, cpus: 8 },
      { role: "build", reachable: true, diskUsedPct: 55, load1m: 2.0, cpus: 16 },
      { role: "test", reachable: true, diskUsedPct: 35, load1m: 0.5, cpus: 8 },
    ],
    ...overrides,
  };
}

describe("cf-quota routing and ceiling monitor", () => {
  test("returns ok (exit 0) and keeps edge/CI on primary targets when all quotas are healthy", () => {
    const res = evaluateQuotaRouting(baseTelemetry());
    expect(res.ok).toBe(true);
    expect(res.severity).toBe("ok");
    expect(res.exitCode).toBe(0);
    expect(res.routing.edgeCompute).toBe("cloudflare-workers");
    expect(res.routing.edgeStatic).toBe("cloudflare-r2");
    expect(res.routing.ciRunner).toBe("github-hosted");
    expect(res.routing.buildTarget).toBe("build");
    expect(res.routing.reasons).toEqual([]);
  });

  test("fails over Cloudflare Workers compute to prod-origin when free 100k/day ceiling is critical", () => {
    const t = baseTelemetry();
    t.cloudflare.dailyWorkerRequests = 95_000;
    const res = evaluateQuotaRouting(t);
    expect(res.severity).toBe("critical");
    expect(res.exitCode).toBe(2);
    expect(res.routing.edgeCompute).toBe("prod-origin");
    expect(res.routing.reasons.some((r) => r.includes("prod-origin"))).toBe(true);
  });

  test("fails over edge compute and static to test-standby when prod host is unhealthy", () => {
    const t = baseTelemetry({
      vpsFleet: [
        { role: "prod", reachable: false },
        { role: "build", reachable: true, diskUsedPct: 45, load1m: 1.0, cpus: 16 },
        { role: "test", reachable: true, diskUsedPct: 42, load1m: 0.8, cpus: 8 },
      ],
    });
    t.cloudflare.dailyWorkerRequests = 99_000;
    t.cloudflare.r2StorageBytes = 12 * GIB;
    const res = evaluateQuotaRouting(t);
    expect(res.severity).toBe("critical");
    expect(res.exitCode).toBe(2);
    expect(res.routing.edgeCompute).toBe("test-standby");
    expect(res.routing.edgeStatic).toBe("test-standby");
  });

  test("offloads GitHub Actions to self-hosted build role when minutes or 10 GiB repo cache crosses 75%", () => {
    const t = baseTelemetry();
    t.githubActions.repos = [{ repo: "aphrody-labs/aphrody", cacheBytes: 8 * GIB }];
    const res = evaluateQuotaRouting(t);
    expect(res.severity).toBe("warn");
    expect(res.exitCode).toBe(1);
    expect(res.routing.ciRunner).toBe("self-hosted-build");
    expect(res.routing.buildTarget).toBe("build");
  });

  test("routes offloaded CI to self-hosted-test when build host disk or load exceeds critical threshold", () => {
    const t = baseTelemetry({
      vpsFleet: [
        { role: "prod", reachable: true, diskUsedPct: 40, load1m: 1.0, cpus: 8 },
        { role: "build", reachable: true, diskUsedPct: 92, load1m: 2.0, cpus: 16 },
        { role: "test", reachable: true, diskUsedPct: 30, load1m: 0.5, cpus: 8 },
      ],
    });
    t.githubActions.monthlyMinutesUsed = 1_650;
    const res = evaluateQuotaRouting(t);
    expect(res.severity).toBe("critical");
    expect(res.exitCode).toBe(2);
    expect(res.routing.ciRunner).toBe("self-hosted-test");
    expect(res.routing.buildTarget).toBe("test");
  });

  test("paid tier treats 15+ workers, >10 GiB R2, and >1M Class A ops as warn headroom without forcing edge failover", () => {
    const t = baseTelemetry();
    t.cloudflare.tier = "paid";
    t.cloudflare.workersCount = 15;
    t.cloudflare.dailyWorkerRequests = 120_000;
    t.cloudflare.r2StorageBytes = 40 * GIB;
    t.cloudflare.r2ClassAMonthly = 3_800_000;
    t.cloudflare.r2ClassBMonthly = 12_000_000;
    const res = evaluateQuotaRouting(t);
    expect(res.severity).toBe("warn");
    expect(res.exitCode).toBe(1);
    expect(res.routing.edgeCompute).toBe("cloudflare-workers");
    expect(res.routing.edgeStatic).toBe("cloudflare-r2");
    expect(res.findings.some((f) => f.metric === "r2_storage_soft" && f.severity === "warn")).toBe(true);
    expect(res.findings.some((f) => f.metric === "workers_count" && f.severity === "warn")).toBe(true);
  });

  test("tracks KV and D1 headroom thresholds accurately", () => {
    const t = baseTelemetry();
    t.cloudflare.kvDailyWrites = DEFAULT_QUOTA_POLICY.cloudflare.kvDailyWritesCeiling;
    t.cloudflare.d1DailyRowsWritten = DEFAULT_QUOTA_POLICY.cloudflare.d1DailyRowsWrittenCeiling;
    const res = evaluateQuotaRouting(t);
    expect(res.severity).toBe("critical");
    expect(res.findings.some((f) => f.metric === "kv_daily_writes" && f.severity === "critical")).toBe(true);
    expect(res.findings.some((f) => f.metric === "d1_daily_rows_written" && f.severity === "critical")).toBe(true);
  });
});
