import { parse as parseJsonc } from "jsonc-parser";
import { z } from "zod";

export const FinOpsPolicySchema = z.object({
  workers_included_requests: z.number().int().positive(),
  workers_overage_per_million_usd: z.number().nonnegative(),
  workers_included_cpu_ms: z.number().int().positive(),
  workers_cpu_per_million_ms_usd: z.number().nonnegative(),
  observability_included_events: z.number().int().positive(),
  observability_overage_per_million_usd: z.number().nonnegative(),
  r2_included_gb: z.number().nonnegative(),
  r2_storage_per_gb_usd: z.number().nonnegative(),
  r2_included_class_a: z.number().int().positive(),
  r2_class_a_per_million_usd: z.number().nonnegative(),
  r2_included_class_b: z.number().int().positive(),
  r2_class_b_per_million_usd: z.number().nonnegative(),
  default_cdn_cpu_ms: z.number().int().min(10).max(30_000),
  default_api_cpu_ms: z.number().int().min(10).max(30_000),
  cdn_log_sampling_rate: z.number().min(0).max(1),
  api_log_sampling_rate: z.number().min(0).max(1),
  default_trace_sampling_rate: z.number().min(0).max(1),
  r2_multipart_abort_seconds: z.number().int().min(3_600).max(604_800),
});

export const WorkerContractSchema = z.object({
  profile: z.enum(["cdn-router", "web-router", "api-compute", "api-gateway", "webhook"]),
  cpu_ms: z.number().int().min(10).max(30_000),
  log_sampling: z.number().min(0).max(1),
  trace_sampling: z.number().min(0).max(1),
  workers_dev: z.boolean(),
  previews: z.boolean(),
  secrets: z.array(z.string()).default([]),
});

export const BucketContractSchema = z.object({
  tier: z.enum(["hot-reusable", "dist", "site-cdn", "cold-offload"]),
  custom_domains: z.array(z.string()).default([]),
  abort_multipart_seconds: z.number().int().min(3_600).max(604_800),
  public_r2_dev: z.boolean(),
});

export const TunnelContractSchema = z.object({
  transport: z.enum(["quic", "http2"]),
  connectors: z.number().int().min(1),
  ingress: z.array(z.string().min(1)).min(1),
});

export const ClusterNodeSchema = z.object({
  provider: z.enum(["ovh", "hostinger"]),
  os: z.string().min(1),
  mesh_ip: z.string().min(7),
  services: z.array(z.string()).min(1),
});

export const VpnExitSchema = z.object({
  id: z.string().min(2).max(8),
  purpose: z.string().min(1),
  country: z.string().length(2),
  city: z.string().min(3).max(4),
  uid: z.number().int().min(1000).max(60_000),
  table: z.number().int().min(51_820).max(60_000),
});

export const HostTargetSchema = z.enum(["host:build", "host:edge_proxy", "host:origin"]);

export const ZoneDirectOriginSchema = z.object({
  name: z.string().min(1),
  type: z.enum(["A", "AAAA"]),
  target: HostTargetSchema,
  proxied: z.boolean(),
  comment: z.string().optional(),
});

export const ZoneStaticCnameSchema = z.object({
  name: z.string().min(1),
  target: z.string().min(1),
  proxied: z.boolean().default(false),
  comment: z.string().optional(),
});

export const ZoneMxRecordSchema = z.object({
  name: z.string().min(1),
  target: z.string().min(1),
  priority: z.number().int().min(0).max(65_535),
  comment: z.string().optional(),
});

export const ZoneCaaRecordSchema = z.object({
  name: z.string().min(1),
  flags: z.number().int().min(0).max(255).default(0),
  tag: z.enum(["issue", "issuewild", "iodef"]),
  value: z.string().min(1),
  comment: z.string().optional(),
});

export const ZoneSrvRecordSchema = z.object({
  name: z.string().min(1),
  priority: z.number().int().min(0).max(65_535),
  weight: z.number().int().min(0).max(65_535),
  port: z.number().int().min(1).max(65_535),
  target: z.string().min(1),
  comment: z.string().optional(),
});

export const ZoneTxtRecordSchema = z.object({
  name: z.string().min(1),
  content: z.string().min(1),
  comment: z.string().optional(),
});

export const ZoneWorkerRouteSchema = z.object({
  pattern: z.string().min(1),
  script: z.string().min(1),
});

export const ZoneWorkerDomainSchema = z.object({
  hostname: z.string().min(1),
  service: z.string().min(1),
});

/** Zone settings the control plane owns (Cloudflare API ids: http3, 0rtt, early_hints, tls_1_3, min_tls_version). `zrt` is TLS 1.3 with 0-RTT. */
export const ZoneSettingsSchema = z.object({
  http3: z.enum(["on", "off"]).optional(),
  zero_rtt: z.enum(["on", "off"]).optional(),
  early_hints: z.enum(["on", "off"]).optional(),
  tls_1_3: z.enum(["on", "off", "zrt"]).optional(),
  min_tls_version: z.enum(["1.0", "1.1", "1.2", "1.3"]).optional(),
});

export type ZoneSettings = z.infer<typeof ZoneSettingsSchema>;

/** Contract key to Cloudflare zone-setting id. */
export const ZONE_SETTING_IDS: Record<keyof ZoneSettings, string> = {
  http3: "http3",
  zero_rtt: "0rtt",
  early_hints: "early_hints",
  tls_1_3: "tls_1_3",
  min_tls_version: "min_tls_version",
};

export const ZoneContractSchema = z.object({
  settings: ZoneSettingsSchema.default({}),
  tunnel_cnames: z.record(z.string(), z.array(z.string())).default({}),
  worker_aaaa: z.array(z.string()).default([]),
  r2_cnames: z.record(z.string(), z.string()).default({}),
  direct_origins: z.array(ZoneDirectOriginSchema).default([]),
  static_cnames: z.array(ZoneStaticCnameSchema).default([]),
  mx_records: z.array(ZoneMxRecordSchema).default([]),
  caa_records: z.array(ZoneCaaRecordSchema).default([]),
  srv_records: z.array(ZoneSrvRecordSchema).default([]),
  txt_records: z.array(ZoneTxtRecordSchema).default([]),
  worker_routes: z.array(ZoneWorkerRouteSchema).default([]),
  worker_domains: z.array(ZoneWorkerDomainSchema).default([]),
});

export type ZoneContract = z.infer<typeof ZoneContractSchema>;

export const SecretsStoreContractSchema = z.object({
  store_name: z.string().min(1),
  quota_limit: z.number().int().min(1).max(1000).default(100),
  scopes: z.array(z.string()).min(1).default(["workers"]),
  required_keys: z.array(z.string()).min(1),
  bundle_overflow_key: z.string().min(1).default("APHRODY_VAULT_BUNDLE_V1"),
  aliases: z.record(z.string(), z.string()).default({}),
});

export type SecretsStoreContract = z.infer<typeof SecretsStoreContractSchema>;

export const AyCfManifestSchema = z.object({
  product: z.object({
    name: z.string().min(1),
    repo: z.string().min(1),
    fork_org: z.string().min(1),
    description: z.string().min(1),
  }),
  plans: z.object({
    workers_paid: z.boolean(),
    workers_paid_base_usd: z.number().nonnegative(),
    r2_paid: z.boolean(),
    observability_paid: z.boolean(),
    images_stream_basic: z.boolean(),
  }),
  finops: FinOpsPolicySchema,
  domains: z.object({
    primary: z.array(z.string()).min(2),
    sites: z.array(z.string()).min(2),
  }),
  site: z.array(
    z.object({
      id: z.string().min(1),
      zone: z.string().min(1),
      kind: z.string().min(1),
      tunnel: z.string().min(1),
      origin_role: z.enum(["edge-proxy", "origin"]),
    }),
  ),
  tunnels: z.record(z.string(), TunnelContractSchema),
  buckets: z.record(z.string(), BucketContractSchema),
  workers: z.record(z.string(), WorkerContractSchema),
  zone_settings: ZoneSettingsSchema.default({}),
  /** Scripts that own Worker routes and custom domains outside apps/edge/edge.toml (other repositories); every other route is the edge Worker's. */
  edge: z.object({ external_scripts: z.array(z.string().min(1)).default([]) }).default({ external_scripts: [] }),
  zones: z.record(z.string(), ZoneContractSchema),
  secrets_store: SecretsStoreContractSchema.optional(),
  cluster: z.object({
    mesh_cidr: z.string().min(9),
    cloudflared_quic_port: z.number().int().min(1).max(65_535),
    build: ClusterNodeSchema,
    edge_proxy: ClusterNodeSchema,
    origin: ClusterNodeSchema,
  }),
  vpn: z.object({
    provider: z.literal("mullvad"),
    mode: z.literal("uid-policy-routing"),
    preserve_mesh_cidr: z.string().min(9),
    exit: z.array(VpnExitSchema).min(1),
  }),
});

export type AyCfManifest = z.infer<typeof AyCfManifestSchema>;

export const WranglerConfigSchema = z.object({
  name: z.string().regex(/^[a-z0-9-]+$/),
  main: z.string().min(1),
  compatibility_date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  workers_dev: z.boolean().optional(),
  preview_urls: z.boolean().optional(),
  limits: z
    .object({
      cpu_ms: z.number().int().positive(),
    })
    .optional(),
  observability: z
    .object({
      enabled: z.boolean(),
      head_sampling_rate: z.number().min(0).max(1).optional(),
      logs: z
        .object({
          enabled: z.boolean(),
          head_sampling_rate: z.number().min(0).max(1).optional(),
        })
        .optional(),
      traces: z
        .object({
          enabled: z.boolean(),
          head_sampling_rate: z.number().min(0).max(1).optional(),
        })
        .optional(),
    })
    .optional(),
  r2_buckets: z
    .array(
      z.object({
        binding: z.string().min(1),
        bucket_name: z.string().min(1),
      }),
    )
    .optional(),
});

export function parseWranglerConfig(raw: string, ext: ".jsonc" | ".json" | ".toml"): z.infer<typeof WranglerConfigSchema> {
  if (ext === ".toml") {
    return WranglerConfigSchema.parse(Bun.TOML.parse(raw));
  }
  const errors: Array<{ error: number; offset: number; length: number }> = [];
  // Relies on our patched jsonc-parser (`allowTrailingComma: true` by default for Wrangler JSONC).
  const parsed = parseJsonc(raw, errors);
  if (errors.length > 0) {
    throw new Error(`JSONC parse failed with ${errors.length} error(s)`);
  }
  return WranglerConfigSchema.parse(parsed);
}

export function auditWranglerManifest(
  raw: string,
  ext: ".jsonc" | ".json" | ".toml",
  expectedWorkersDev = false,
): string[] {
  const cfg = parseWranglerConfig(raw, ext);
  const issues: string[] = [];
  if (cfg.compatibility_date < "2026-01-01") {
    issues.push(`${cfg.name}: compatibility_date ${cfg.compatibility_date} is stale (< 2026-01-01)`);
  }
  if (!cfg.observability?.enabled) {
    issues.push(`${cfg.name}: observability.enabled must be true`);
  }
  if (!cfg.observability?.traces?.enabled) {
    issues.push(`${cfg.name}: observability.traces.enabled must be true`);
  }
  if (!cfg.limits?.cpu_ms) {
    issues.push(`${cfg.name}: limits.cpu_ms must be set explicitly to bound Workers Paid CPU billing`);
  }
  if (cfg.workers_dev !== expectedWorkersDev) {
    issues.push(`${cfg.name}: workers_dev=${String(cfg.workers_dev)} does not match cf.toml (${expectedWorkersDev})`);
  }
  return issues;
}
