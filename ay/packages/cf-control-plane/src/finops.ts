import type { AyCfManifest } from "./schemas.ts";

export interface UsageInput {
  requests: number;
  cpuMsTotal: number;
  observabilityEvents: number;
  r2StorageGb: number;
  r2ClassAOps: number;
  r2ClassBOps: number;
}

export interface CostBreakdown {
  workersBaseUsd: number;
  workersOverageUsd: number;
  observabilityOverageUsd: number;
  r2StorageUsd: number;
  r2OpsUsd: number;
  totalUsd: number;
}

export function estimateMonthlyCost(manifest: AyCfManifest, usage: UsageInput): CostBreakdown {
  const f = manifest.finops;
  const extraReq = Math.max(0, usage.requests - f.workers_included_requests);
  const extraCpu = Math.max(0, usage.cpuMsTotal - f.workers_included_cpu_ms);
  const workersOverageUsd =
    (extraReq / 1_000_000) * f.workers_overage_per_million_usd +
    (extraCpu / 1_000_000) * f.workers_cpu_per_million_ms_usd;

  const extraObs = Math.max(0, usage.observabilityEvents - f.observability_included_events);
  const observabilityOverageUsd = (extraObs / 1_000_000) * f.observability_overage_per_million_usd;

  const extraGb = Math.max(0, usage.r2StorageGb - f.r2_included_gb);
  const r2StorageUsd = extraGb * f.r2_storage_per_gb_usd;

  const extraA = Math.max(0, usage.r2ClassAOps - f.r2_included_class_a);
  const extraB = Math.max(0, usage.r2ClassBOps - f.r2_included_class_b);
  const r2OpsUsd =
    (extraA / 1_000_000) * f.r2_class_a_per_million_usd +
    (extraB / 1_000_000) * f.r2_class_b_per_million_usd;

  const workersBaseUsd = manifest.plans.workers_paid_base_usd;
  const round4 = (n: number) => Math.round(n * 10_000) / 10_000;
  return {
    workersBaseUsd: round4(workersBaseUsd),
    workersOverageUsd: round4(workersOverageUsd),
    observabilityOverageUsd: round4(observabilityOverageUsd),
    r2StorageUsd: round4(r2StorageUsd),
    r2OpsUsd: round4(r2OpsUsd),
    totalUsd: round4(workersBaseUsd + workersOverageUsd + observabilityOverageUsd + r2StorageUsd + r2OpsUsd),
  };
}

export interface BucketUsageRow {
  name: string;
  tier: "hot-reusable" | "dist" | "site-cdn" | "cold-offload";
  bytes: number;
  objects: number;
}

export interface FinOpsReport {
  totalR2Gb: number;
  hotR2Gb: number;
  coldOffloadCandidateGb: number;
  baselineBill: CostBreakdown;
  optimizedBill: CostBreakdown;
  monthlySavingsUsd: number;
  recommendations: string[];
}

export function buildFinOpsReport(
  manifest: AyCfManifest,
  bucketUsage: BucketUsageRow[],
  estimatedMonthlyReq = 12_000_000,
): FinOpsReport {
  const gb = (b: number) => Math.round((b / 1_000_000_000) * 1000) / 1000;
  const totalBytes = bucketUsage.reduce((acc, row) => acc + row.bytes, 0);
  const coldBytes = bucketUsage
    .filter((row) => row.tier === "cold-offload" || row.name === "aphrody-dist" || row.name === "shenron-cdn")
    .reduce((acc, row) => acc + row.bytes, 0);
  const hotBytes = Math.max(0, totalBytes - coldBytes);

  // Before: 100% log + invocation log events (~2 events/req across 12M req + cron/queues = 30M events), 68.5GB R2, uncached Class B
  const baselineBill = estimateMonthlyCost(manifest, {
    requests: estimatedMonthlyReq,
    cpuMsTotal: 20_000_000,
    observabilityEvents: Math.max(25_000_000, estimatedMonthlyReq * 2.5),
    r2StorageGb: gb(totalBytes),
    r2ClassAOps: 900_000,
    r2ClassBOps: Math.round(estimatedMonthlyReq * 1.1),
  });

  // After: 10% CDN log sampling + 5% trace sampling, R2 Custom Domain 1-year edge cache (85% offload), cold archives on OVH Garage S3
  const optimizedReq = Math.round(estimatedMonthlyReq * 0.45);
  const optimizedBill = estimateMonthlyCost(manifest, {
    requests: optimizedReq,
    cpuMsTotal: 9_000_000,
    observabilityEvents: Math.round(optimizedReq * 0.25),
    r2StorageGb: gb(Math.min(hotBytes, 9_500_000_000)),
    r2ClassAOps: 450_000,
    r2ClassBOps: Math.round(estimatedMonthlyReq * 0.15),
  });

  return {
    totalR2Gb: gb(totalBytes),
    hotR2Gb: gb(hotBytes),
    coldOffloadCandidateGb: gb(coldBytes),
    baselineBill,
    optimizedBill,
    monthlySavingsUsd: Math.round((baselineBill.totalUsd - optimizedBill.totalUsd) * 100) / 100,
    recommendations: [
      "Cap CDN/router Workers at cpu_ms=50 and API/MCP Workers at cpu_ms=250-500.",
      "Sample CDN/router logs at 0.1 and traces at 0.05 to remain under 20M free monthly Observability events.",
      "Disable public .workers.dev and preview URLs on production Workers with custom domains/routes/queues.",
      "Tighten R2 incomplete multipart abort lifecycle rule from 7d (604800s) to 24h (86400s) across all 11 buckets.",
      "Add R2 Custom Domains (releases/downloads/video/bun-lib/r2) to Zone Edge Cache Rulesets with Origin Range Requests.",
      "Offload cold buckets (aphrody-backups, aphrody-chromium, aphrody-gnu, aphrody-lore) to OVH edge_proxy Garage S3 to stay under R2 10 GB free tier.",
      "Remove wildcard *.aphrody.app/* interception over R2 Custom Domains and dual owner conflict on downloads.aphrody.com.",
    ],
  };
}
