#!/usr/bin/env bun
// Workers Paid + R2 Paid FinOps live cost & savings reporter for ay-cf.
import { join, resolve } from "node:path";
import {
  AyCfManifestSchema,
  buildFinOpsReport,
  captureLiveSnapshot,
  createCloudflareClient,
  type BucketUsageRow,
} from "../packages/cf-control-plane/src/index.ts";
import { privateDir as privateRoot } from "./local.ts";

const root = resolve(import.meta.dir, "..");
const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(join(root, "packages/cf-control-plane/cf.toml")).text()));
const localFile = Bun.file(join(root, "cf.local.toml"));
let accountId = Bun.env.CLOUDFLARE_ACCOUNT_ID ?? "";
if (!accountId && (await localFile.exists())) {
  const localCfg = Bun.TOML.parse(await localFile.text()) as { cloudflare?: { account_id?: string } };
  accountId = localCfg.cloudflare?.account_id ?? "";
}

let bucketRows: BucketUsageRow[];
let source = "manifest-fallback";

if (Bun.env.CLOUDFLARE_API_TOKEN && accountId) {
  const client = createCloudflareClient();
  const snap = await captureLiveSnapshot(client, accountId);
  bucketRows = snap.buckets.map((b) => ({
    name: b.name,
    tier: manifest.buckets[b.name]?.tier ?? "dist",
    bytes: b.metrics.bytes,
    objects: b.metrics.objects,
  }));
  source = "cloudflare-live-api";
} else {
  bucketRows = Object.entries(manifest.buckets).map(([name, spec]) => ({
    name,
    tier: spec.tier,
    bytes: spec.tier === "hot-reusable" ? 900_000_000 : spec.tier === "cold-offload" ? 11_000_000_000 : 18_000_000_000,
    objects: 100,
  }));
}

const report = buildFinOpsReport(manifest, bucketRows);
const privateDir = privateRoot(root);
await Bun.write(join(privateDir, "finops-report.json"), JSON.stringify({ source, buckets: bucketRows, report }, null, 2) + "\n");

const md = [
  "# ay-cf Cloudflare FinOps Report",
  "",
  `- Source: \`${source}\``,
  `- Total R2 Storage: **${report.totalR2Gb.toFixed(2)} GB** (hot reusable: ${report.hotR2Gb.toFixed(2)} GB, cold-offload / dist candidate: ${report.coldOffloadCandidateGb.toFixed(2)} GB)`,
  `- Baseline Estimated Monthly Bill: **$${report.baselineBill.totalUsd.toFixed(2)}/mo** (Workers base $${report.baselineBill.workersBaseUsd.toFixed(2)}, Observability $${report.baselineBill.observabilityOverageUsd.toFixed(2)}, R2 storage $${report.baselineBill.r2StorageUsd.toFixed(2)}, R2 ops $${report.baselineBill.r2OpsUsd.toFixed(2)})`,
  `- Optimized Target Monthly Bill: **$${report.optimizedBill.totalUsd.toFixed(2)}/mo** (saves **$${report.monthlySavingsUsd.toFixed(2)}/mo**)`,
  "",
  "## Buckets",
  "",
  "| Bucket | Tier | Objects | Size (GB) |",
  "| :--- | :--- | ---: | ---: |",
  ...bucketRows
    .sort((a, b) => b.bytes - a.bytes)
    .map((b) => `| ${b.name} | ${b.tier} | ${b.objects} | ${(b.bytes / 1e9).toFixed(3)} |`),
  "",
  "## Recommendations",
  "",
  ...report.recommendations.map((r) => `- ${r}`),
  "",
].join("\n");
await Bun.write(join(privateDir, "finops-report.md"), md);
console.log(`FinOps (${source}): total=${report.totalR2Gb.toFixed(2)} GB, baseline=$${report.baselineBill.totalUsd.toFixed(2)}/mo -> optimized=$${report.optimizedBill.totalUsd.toFixed(2)}/mo (saves $${report.monthlySavingsUsd.toFixed(2)}/mo)`);
