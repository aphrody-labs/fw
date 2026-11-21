#!/usr/bin/env bun
// Validates packages/cf-control-plane/r2-catalog.toml SPDX compliance and optionally probes upstream public APIs (--probe-live).
import { join, resolve } from "node:path";
import { loadR2Catalog } from "../packages/cf-control-plane/src/index.ts";

const root = resolve(import.meta.dir, "..");
const catalog = loadR2Catalog(await Bun.file(join(root, "packages/cf-control-plane/r2-catalog.toml")).text());
console.log(
  `R2 Catalog OK: ${catalog.rust_crate.length} pure-Rust crates, ${catalog.source.length} permissive sources (${catalog.meta.assets_origin}, ${catalog.meta.video_origin})`,
);

if (Bun.argv.includes("--probe-live")) {
  const [phHdri, phTex, phMod, acg, noto] = await Promise.all([
    fetch("https://api.polyhaven.com/assets?t=hdris").then((r) => r.json() as Promise<Record<string, unknown>>),
    fetch("https://api.polyhaven.com/assets?t=textures").then((r) => r.json() as Promise<Record<string, unknown>>),
    fetch("https://api.polyhaven.com/assets?t=models").then((r) => r.json() as Promise<Record<string, unknown>>),
    fetch("https://ambientcg.com/api/v3/assets?type=material&limit=1").then(
      (r) => r.json() as Promise<{ numberOfResults?: number }>,
    ),
    fetch("https://googlefonts.github.io/noto-emoji-animation/data/api.json").then(
      (r) => r.json() as Promise<{ icons?: unknown[] }>,
    ),
  ]);
  console.log("Live catalog counts:", {
    polyhaven_hdris: Object.keys(phHdri).length,
    polyhaven_textures: Object.keys(phTex).length,
    polyhaven_models: Object.keys(phMod).length,
    ambientcg_materials: acg.numberOfResults ?? 0,
    noto_animated_emoji: noto.icons?.length ?? 0,
  });
}
