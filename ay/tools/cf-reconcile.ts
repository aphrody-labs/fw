#!/usr/bin/env bun
// `--zone <name>` (repeatable) limits the run to those zones of cf.toml: the other zones are neither diffed nor written.
// Reconciles declarative cf.toml against live Cloudflare API (dry-run by default; --backup; --apply-safe; --apply-zones; --apply-settings; --apply-all).
// --apply-settings writes only the zone settings (never DNS or routes); --apply-zones and --apply-all include them. The settings are the zone settings of cf.toml [zone_settings] (http3, 0rtt, early_hints, tls_1_3, min_tls_version) that differ live.
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
const applySettings = applyZones || argv.includes("--apply-settings");
const pruneDns = argv.includes("--prune");
const onlyZones = argv.flatMap((a, i) => (a === "--zone" && argv[i + 1] ? [argv[i + 1]!] : []));

const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(join(root, "packages/cf-control-plane/cf.toml")).text()));
if (onlyZones.length) {
  for (const z of onlyZones) if (!manifest.zones[z]) throw new Error(`--zone ${z}: not a zone of cf.toml`);
  for (const z of Object.keys(manifest.zones)) if (!onlyZones.includes(z)) delete manifest.zones[z];
}
const localCfg = Bun.TOML.parse(await Bun.file(join(root, "cf.local.toml")).text()) as LocalCloudflareContext;
const accountId = localCfg.cloudflare?.account_id ?? Bun.env.CLOUDFLARE_ACCOUNT_ID;
if (!accountId) throw new Error("Run `bun tools/cf-bootstrap-local.ts` first (missing account_id)");

const client = createCloudflareClient();
const privateDir = privateRoot(root);
mkdirSync(privateDir, { recursive: true });

let snap = await captureLiveSnapshot(client, accountId);
if (backupOnly || applySafe || applyZones || applySettings) {
  const backupFile = join(privateDir, `live-backup-${new Date().toISOString().slice(0, 10)}.json`);
  writeFileSync(backupFile, JSON.stringify(snap, null, 2) + "\n", { mode: 0o600 });
  console.log(`Saved live backup -> ${backupFile}`);
}
if (backupOnly) process.exit(0);

const zoneReconcile = await applyZoneReconciliation(client, manifest, snap, localCfg, {
  apply: applyZones,
  prune: pruneDns,
  settings: applySettings,
});
const summary = await applySafeOptimizations(client, manifest, snap, { apply: applySafe });

if (applySafe || applyZones || applySettings) {
  snap = await captureLiveSnapshot(client, accountId);
}
const conflicts = detectStructuralConflicts(snap);
const postZoneCheck = await applyZoneReconciliation(client, manifest, snap, localCfg, { apply: false });

const md = [
  `# ay-cf Reconciliation Report (${applySafe || applyZones || applySettings ? "APPLIED" : "DRY-RUN"})`,
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
  `- Routes and domains skipped because their script is not deployed yet: ${zoneReconcile.skippedUndeployed.length}${zoneReconcile.skippedUndeployed.length ? ` (${zoneReconcile.skippedUndeployed.join(", ")})` : ""}`,
  `- Zone settings (http3, 0rtt, early_hints, tls_1_3, min_tls_version) applied: ${zoneReconcile.appliedSettings}`,
  `- Remaining structural conflicts: ${conflicts.length}`,
  "",
  "## Multi-Zone Cartography Parity (188 DNS Records)",
  "",
  "| Zone | Expected DNS | Live DNS | DNS Drift (+/~/-) | Routes Drift (+/~/-) | Domains Drift (+/-) | Settings drift |",
  "| :--- | ---: | ---: | :--- | :--- | :--- | :--- |",
  ...postZoneCheck.zoneDiffs.map(
    (d) =>
      `| \`${d.zone}\` | ${d.expectedDnsCount} | ${d.liveDnsCount} | +${d.dnsToCreate.length} / ~${d.dnsToUpdate.length} / -${d.dnsToDelete.length} | +${d.routesToCreate.length} / ~${d.routesToUpdate.length} / -${d.routesToDelete.length} | +${d.domainsToCreate.length} / -${d.domainsToDelete.length} | ${d.settingsToUpdate.map((c) => `${c.id} ${c.live ?? "?"}->${c.want}`).join(", ") || "none"} |`,
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
  `Reconcile (${applySafe || applyZones || applySettings ? "APPLIED" : "DRY-RUN"}): dns_updates=${zoneReconcile.appliedDnsUpdates}, route_creates=${zoneReconcile.appliedRouteCreates}, route_deletes=${zoneReconcile.appliedRouteDeletes}, domain_deletes=${zoneReconcile.appliedDomainDeletes}, cache=${summary.cacheRulesetUpdates.length}, conflicts=${conflicts.length}, settings_applied=${zoneReconcile.appliedSettings}, skipped_undeployed=${zoneReconcile.skippedUndeployed.length}`,
);
for (const d of postZoneCheck.zoneDiffs) {
  console.log(
    `  zone=${d.zone.padEnd(16)} dns=${d.liveDnsCount}/${d.expectedDnsCount} drift=(+${d.dnsToCreate.length}/~${d.dnsToUpdate.length}/-${d.dnsToDelete.length}) routes_drift=(+${d.routesToCreate.length}/~${d.routesToUpdate.length}/-${d.routesToDelete.length}) domains_drift=(+${d.domainsToCreate.length}/-${d.domainsToDelete.length}) settings_drift=${d.settingsToUpdate.map((c) => `${c.id}:${c.live ?? "?"}->${c.want}`).join(",") || "none"}`,
  );
}
