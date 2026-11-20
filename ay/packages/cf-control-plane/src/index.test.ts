import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";
import {
  AyCfManifestSchema,
  auditWranglerManifest,
  buildFinOpsReport,
  diffZoneSettings,
  effectiveZoneSettings,
  expandZoneRecords,
  generateClusterConfigs,
  loadR2Catalog,
  parseWranglerConfig,
} from "./index.ts";

const root = resolve(import.meta.dir, "../../..");

describe("@aphrody/ay-cf-control-plane", () => {
  test("validates cf.toml and FinOps savings calculation", async () => {
    const raw = Bun.TOML.parse(await Bun.file(resolve(root, "packages/cf-control-plane/cf.toml")).text());
    const manifest = AyCfManifestSchema.parse(raw);
    expect(Object.keys(manifest.tunnels)).toEqual(["aphrody", "nie", "rosegriffon", "shenron"]);
    expect(Object.keys(manifest.buckets).length).toBe(11);

    const report = buildFinOpsReport(manifest, [
      { name: "aphrody-assets", tier: "hot-reusable", bytes: 868_000_000, objects: 743 },
      { name: "aphrody-dist", tier: "dist", bytes: 18_300_000_000, objects: 40_986 },
      { name: "aphrody-backups", tier: "cold-offload", bytes: 5_860_000_000, objects: 104 },
    ]);
    expect(report.optimizedBill.totalUsd).toBeLessThan(report.baselineBill.totalUsd);
    expect(report.monthlySavingsUsd).toBeGreaterThan(5);
  });

  test("every zone declares full HTTP/3: http3, 0rtt, tls_1_3 zrt, early_hints; drift is diffed per setting id", async () => {
    const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(resolve(root, "packages/cf-control-plane/cf.toml")).text()));
    for (const zone of Object.keys(manifest.zones)) {
      expect(effectiveZoneSettings(manifest, zone)).toMatchObject({ http3: "on", zero_rtt: "on", tls_1_3: "zrt", early_hints: "on", min_tls_version: "1.2" });
    }
    const live = { http3: "off", "0rtt": "on", early_hints: "on", tls_1_3: "on", min_tls_version: "1.2" };
    expect(diffZoneSettings(effectiveZoneSettings(manifest, "aphrody.com"), live)).toEqual([
      { id: "http3", live: "off", want: "on" },
      { id: "tls_1_3", live: "on", want: "zrt" },
    ]);
    expect(diffZoneSettings({ http3: "on" }, { http3: "on" })).toEqual([]);
  });

  test("expands declarative multi-zone cartography to exact 82 + 60 + 20 + 0 (rosegriffon.fr is external) = 162 DNS records and zero conflicts", async () => {
    const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(resolve(root, "packages/cf-control-plane/cf.toml")).text()));
    const aphrodyCom = expandZoneRecords("aphrody.com", manifest.zones["aphrody.com"]!);
    const aphrodyApp = expandZoneRecords("aphrody.app", manifest.zones["aphrody.app"]!);
    const dragonballfr = expandZoneRecords("dragonballfr.com", manifest.zones["dragonballfr.com"]!);
    const rosegriffon = expandZoneRecords("rosegriffon.fr", manifest.zones["rosegriffon.fr"]!);

    expect(aphrodyCom.length).toBe(82);
    expect(aphrodyApp.length).toBe(60);
    expect(dragonballfr.length).toBe(20);
    expect(rosegriffon.length).toBe(0);
    expect(aphrodyCom.length + aphrodyApp.length + dragonballfr.length + rosegriffon.length).toBe(162);

    // Verify conflict fixes are baked into declarative state
    expect(manifest.zones["aphrody.com"]!.worker_domains.some((d) => d.hostname === "auto.aphrody.com" && d.service === "ay-edge")).toBe(true);
    expect(manifest.zones["aphrody.com"]!.tunnel_cnames.aphrody).not.toContain("auto");
    expect(manifest.zones["aphrody.com"]!.worker_domains.some((d) => d.hostname === "downloads.aphrody.com")).toBe(false);
    expect(manifest.zones["aphrody.app"]!.worker_routes.some((r) => r.pattern === "*.aphrody.app/*")).toBe(false);
    expect(dragonballfr.find((r) => r.name === "tv.dragonballfr.com")?.proxied).toBe(true);
    expect(manifest.secrets_store?.store_name).toBe("default-secrets-store");
    expect(manifest.secrets_store?.required_keys.length).toBeGreaterThanOrEqual(120);
  });

  test("validates packages/cf-control-plane/r2-catalog.toml and rejects non-permissive SPDX overrides", async () => {
    const raw = await Bun.file(resolve(root, "packages/cf-control-plane/r2-catalog.toml")).text();
    const catalog = loadR2Catalog(raw);
    expect(catalog.source.length).toBeGreaterThanOrEqual(15);
    expect(catalog.rust_crate.length).toBeGreaterThanOrEqual(18);

    const badToml = raw.replaceAll('spdx = "MIT"', 'spdx = "CC-BY-NC-4.0"');
    expect(() => loadR2Catalog(badToml)).toThrow(/blocked SPDX/i);
  });

  test("parses Wrangler JSONC with trailing commas via Bun.JSONC", async () => {
    const wranglerRaw = await Bun.file(resolve(root, "apps/cf-gateway/wrangler.jsonc")).text();
    const parsed = parseWranglerConfig(wranglerRaw, ".jsonc");
    expect(parsed.name).toBe("ay-cf-gateway");
    expect(parsed.limits?.cpu_ms).toBe(50);
    expect(auditWranglerManifest(wranglerRaw, ".jsonc")).toEqual([]);
  });

  test("generates OVH Pingora, Hostinger nftables, Mullvad UID routes, and cloudflared configs without IP leaks", async () => {
    const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(resolve(root, "packages/cf-control-plane/cf.toml")).text()));
    const cfg = generateClusterConfigs(manifest);
    expect(cfg.pingoraYaml).toContain("10.200.77.5:443");
    expect(cfg.nftablesOriginConf).toContain("policy drop");
    expect(cfg.mullvadUidScript).toContain("ip rule add uidrange 2002-2002 lookup 51822 priority 100");
    expect(cfg.cloudflaredHaYaml).toContain("dragonballfr.com");
  });
});
