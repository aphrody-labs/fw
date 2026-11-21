#!/usr/bin/env bun
// Offline CI/dev audit: validates cf.toml, packages/cf-control-plane/r2-catalog.toml, wrangler.jsonc.
import { join, resolve } from "node:path";
import {
  auditWranglerManifest,
  AyCfManifestSchema,
  loadR2Catalog,
} from "../packages/cf-control-plane/src/index.ts";

const root = resolve(import.meta.dir, "..");
const errors: string[] = [];

let manifestWorkers: Record<string, { workers_dev: boolean }> = {};
try {
  const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(join(root, "packages/cf-control-plane/cf.toml")).text()));
  manifestWorkers = manifest.workers;
} catch (err) {
  errors.push(`cf.toml invalid: ${(err as Error).message}`);
}

try {
  loadR2Catalog(await Bun.file(join(root, "packages/cf-control-plane/r2-catalog.toml")).text());
} catch (err) {
  errors.push(`packages/cf-control-plane/r2-catalog.toml invalid: ${(err as Error).message}`);
}

for (const relPath of ["apps/cf-gateway/wrangler.jsonc", "apps/tools-release/wrangler.jsonc"]) {
  const wf = Bun.file(join(root, relPath));
  if (!(await wf.exists())) {
    errors.push(`Missing ${relPath}`);
  } else {
    const raw = await wf.text();
    const parsedName = /"name"\s*:\s*"([^"]+)"/.exec(raw)?.[1] ?? "";
    const expectedWorkersDev = manifestWorkers[parsedName]?.workers_dev ?? false;
    errors.push(...auditWranglerManifest(raw, ".jsonc", expectedWorkersDev));
  }
}

if (errors.length) {
  for (const e of errors) console.error(`AUDIT FAIL: ${e}`);
  process.exit(1);
}
console.log(
  "Audit passed: cf.toml, packages/cf-control-plane/r2-catalog.toml, wrangler.jsonc (ay-cf-gateway, ay-tools-release) verified.",
);
