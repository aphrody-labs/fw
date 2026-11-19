#!/usr/bin/env bun
// Reconciles declarative cf.toml against live Cloudflare API (dry-run by default; --backup; --apply-safe; --apply-zones; --apply-all).
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  applySafeOptimizations,
  applyZoneReconciliation,
  AyCfManifestSchema,
  captureLiveSnapshot,
  createCloudflareClient,
  detectStructuralConflicts,
  type LocalCloudflareContext,
} from "../packages/cf-control-plane/src/index.ts";
import { privateDir as privateRoot } from "./local.ts";

const root = resolve(import.meta.dir, "..");
const argv = Bun.argv.slice(2);
const backupOnly = argv.includes("--backup");
const applyAll = argv.includes("--apply-all") || argv.includes("--apply");
const applySafe = applyAll || argv.includes("--apply-safe");
const applyZones = applyAll || argv.includes("--apply-zones");
const pruneDns = argv.includes("--prune");

const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(join(root, "packages/cf-control-plane/cf.toml")).text()));
const localCfg = Bun.TOML.parse(await Bun.file(join(root, "cf.local.toml")).text()) as LocalCloudflareContext;
const accountId = localCfg.cloudflare?.account_id ?? Bun.env.CLOUDFLARE_ACCOUNT_ID;
if (!accountId) throw new Error("Run `bun tools/cf-bootstrap-local.ts` first (missing account_id)");

const client = createCloudflareClient();
const privateDir = privateRoot(root);
mkdirSync(privateDir, { recursive: true });

let snap = await captureLiveSnapshot(client, accountId);
if (backupOnly || applySafe || applyZones) {
  const backupFile = join(privateDir, `live-backup-${new Date().toISOString().slice(0, 10)}.json`);
  writeFileSync(backupFile, JSON.stringify(snap, null, 2) + "\n", { mode: 0o600 });
  console.log(`Saved live backup -> ${backupFile}`);
}
if (backupOnly) process.exit(0);

const zoneReconcile = await applyZoneReconciliation(client, manifest, snap, localCfg, {
  apply: applyZones,
  prune: pruneDns,
});
const summary = await applySafeOptimizations(client, manifest, snap, { apply: applySafe });

if (applySafe || applyZones) {
  snap = await captureLiveSnapshot(client, accountId);
}
const conflicts = detectStructuralConflicts(snap);
const postZoneCheck = await applyZoneReconciliation(client, manifest, snap, localCfg, { apply: false });

const md = [
  `# ay-cf Reconciliation Report (${applySafe || applyZones ? "APPLIED" : "DRY-RUN"})`,
  "",
  `- Workers subdomain (.workers.dev / previews) updates: ${summary.subdomainUpdates.length}`,
  `- Workers observability right-sizing updates: ${summary.observabilityUpdates.length}`,
  `- Workers CPU limits (cpu_ms) updates: ${summary.limitsUpdates.length}`,
  `- R2 multipart-abort 86400s lifecycle updates: ${summary.r2LifecycleUpdates.length}`,
  `- R2 Range/WebGPU CORS updates: ${summary.r2CorsUpdates.length}`,
  `- R2 managed r2.dev updates: ${summary.r2DevUpdates.length}`,
  `- Zone Cache Ruleset optimizations: ${summary.cacheRulesetUpdates.length}`,
  `- Zone DNS updates applied: +${zoneReconcile.appliedDnsCreates} / ~${zoneReconcile.appliedDnsUpdates} / -${zoneReconcile.appliedDnsDeletes}`,
  `- Worker Routes updates applied: +${zoneReconcile.appliedRouteCreates} / ~${zoneReconcile.appliedRouteUpdates} / -${zoneReconcile.appliedRouteDeletes}`,
  `- Worker Custom Domains updates applied: +${zoneReconcile.appliedDomainCreates} / -${zoneReconcile.appliedDomainDeletes}`,
  `- Remaining structural conflicts: ${conflicts.length}`,
  "",
  "## Multi-Zone Cartography Parity (188 DNS Records)",
  "",
  "| Zone | Expected DNS | Live DNS | DNS Drift (+/~/-) | Routes Drift (+/~/-) | Domains Drift (+/-) |",
  "| :--- | ---: | ---: | :--- | :--- | :--- |",
  ...postZoneCheck.zoneDiffs.map(
    (d) =>
      `| \`${d.zone}\` | ${d.expectedDnsCount} | ${d.liveDnsCount} | +${d.dnsToCreate.length} / ~${d.dnsToUpdate.length} / -${d.dnsToDelete.length} | +${d.routesToCreate.length} / ~${d.routesToUpdate.length} / -${d.routesToDelete.length} | +${d.domainsToCreate.length} / -${d.domainsToDelete.length} |`,
  ),
  "",
  "## Structural Conflicts",
  "",
  ...(conflicts.length
    ? conflicts.map((c) => `- **[${c.kind}]** \`${c.zone}\` / \`${c.hostname}\`: ${c.detail}`)
    : ["- **0 structural conflicts detected across all 4 zones.**"]),
  "",
].join("\n");
writeFileSync(join(privateDir, "reconcile-report.md"), md);
writeFileSync(
  join(privateDir, "reconcile-summary.json"),
  JSON.stringify({ applySafe, applyZones, conflicts, summary, zoneReconcile, postZoneCheck }, null, 2) + "\n",
);

console.log(
  `Reconcile (${applySafe || applyZones ? "APPLIED" : "DRY-RUN"}): dns_updates=${zoneReconcile.appliedDnsUpdates}, route_creates=${zoneReconcile.appliedRouteCreates}, route_deletes=${zoneReconcile.appliedRouteDeletes}, domain_deletes=${zoneReconcile.appliedDomainDeletes}, cache=${summary.cacheRulesetUpdates.length}, conflicts=${conflicts.length}`,
);
for (const d of postZoneCheck.zoneDiffs) {
  console.log(
    `  zone=${d.zone.padEnd(16)} dns=${d.liveDnsCount}/${d.expectedDnsCount} drift=(+${d.dnsToCreate.length}/~${d.dnsToUpdate.length}/-${d.dnsToDelete.length}) routes_drift=(+${d.routesToCreate.length}/~${d.routesToUpdate.length}/-${d.routesToDelete.length}) domains_drift=(+${d.domainsToCreate.length}/-${d.domainsToDelete.length})`,
  );
}
