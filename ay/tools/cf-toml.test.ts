import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { AyCfManifestSchema, expandZoneRecords } from "../packages/cf-control-plane/src/index.ts";

// A cf.toml that does not parse made edge-deploy skip and let a deploy remove live routes: parse and validate it on every `bun run check`.
const raw = readFileSync(join(import.meta.dir, "../packages/cf-control-plane/cf.toml"), "utf8");

test("cf.toml parses and satisfies the manifest schema", () => {
  expect(() => AyCfManifestSchema.parse(Bun.TOML.parse(raw))).not.toThrow();
});

test("no host is declared twice with the same record type in one zone, and an external zone declares nothing", () => {
  const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(raw));
  for (const [zone, spec] of Object.entries(manifest.zones)) {
    const records = expandZoneRecords(zone, spec, {});
    if (spec.external) {
      expect(records).toEqual([]);
      expect(spec.worker_routes).toEqual([]);
      continue;
    }
    const keys = records.map((r) => `${r.type} ${r.name}`);
    const dup = keys.filter((k, i) => keys.indexOf(k) !== i && !k.startsWith("TXT ") && !k.startsWith("MX ") && !k.startsWith("CAA ") && !k.startsWith("SRV "));
    expect(dup).toEqual([]);
  }
});
