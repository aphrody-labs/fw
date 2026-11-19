import { z } from "zod";

export const RustCrateSpecSchema = z.object({
  domain: z.string().min(1),
  crate: z.string().min(1),
  version: z.string().min(1),
  spdx: z.string().min(1),
  wasm32: z.boolean(),
  features: z.array(z.string()).default([]),
  note: z.string().min(1),
});

export const AssetSourceSpecSchema = z.object({
  id: z.string().min(1),
  category: z.enum(["icon", "ui", "hdri", "loop", "lottie", "tex", "glb"]),
  spdx: z.string().min(1),
  attribution_required: z.boolean(),
  endpoint: z.string().url(),
  format: z.string().min(1),
  range_seekable: z.boolean(),
  trademark_guard: z.boolean(),
  note: z.string().min(1),
});

export const R2CatalogManifestSchema = z.object({
  meta: z.object({
    schema_version: z.literal(1),
    assets_bucket: z.string().min(1),
    assets_origin: z.string().url(),
    video_bucket: z.string().min(1),
    video_origin: z.string().url(),
    edge_cache_max_bytes: z.number().int().positive(),
    immutable_cache_control: z.string().min(1),
    index_cache_control: z.string().min(1),
  }),
  policy: z.object({
    allowed_spdx_tier_a: z.array(z.string()).min(1),
    allowed_spdx_tier_b: z.array(z.string()).min(1),
    blocked_spdx_substrings: z.array(z.string()).min(1),
    quarantined_sources: z.array(z.string()).min(1),
  }),
  rust_crate: z.array(RustCrateSpecSchema).min(1),
  source: z.array(AssetSourceSpecSchema).min(1),
});

export type R2CatalogManifest = z.infer<typeof R2CatalogManifestSchema>;

export function loadR2Catalog(tomlText: string): R2CatalogManifest {
  const parsed = R2CatalogManifestSchema.parse(Bun.TOML.parse(tomlText));
  const allowed = new Set([...parsed.policy.allowed_spdx_tier_a, ...parsed.policy.allowed_spdx_tier_b]);

  for (const rc of parsed.rust_crate) {
    for (const blocked of parsed.policy.blocked_spdx_substrings) {
      if (rc.spdx.includes(blocked)) {
        throw new Error(`rust_crate ${rc.crate} carries blocked SPDX license marker "${blocked}": ${rc.spdx}`);
      }
    }
  }
  for (const src of parsed.source) {
    for (const blocked of parsed.policy.blocked_spdx_substrings) {
      if (src.spdx.includes(blocked)) {
        throw new Error(`source ${src.id} carries blocked SPDX license marker "${blocked}": ${src.spdx}`);
      }
    }
    if (!allowed.has(src.spdx)) {
      throw new Error(`source ${src.id} SPDX "${src.spdx}" is outside Tier A/B allowlist`);
    }
    if (src.spdx.startsWith("CC-BY-") && !src.attribution_required) {
      throw new Error(`source ${src.id} uses ${src.spdx} so attribution_required must be true`);
    }
  }
  return parsed;
}
