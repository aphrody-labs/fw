import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { validate } from "./cloud-map.ts";

const tree = (map: string, products: string[], pages: string[]) => {
  const dir = mkdtempSync(join(tmpdir(), "cloud-map-"));
  mkdirSync(join(dir, "docs/research"), { recursive: true });
  writeFileSync(join(dir, "ay.toml"), "");
  writeFileSync(join(dir, "docs/research/cloudflare.toml"), products.map((n) => `[[product]]\nname = "${n}"\n`).join("\n"));
  writeFileSync(join(dir, "docs/research/bun-cloudflare.toml"), map);
  const bun = join(dir, "bun");
  for (const p of pages) {
    mkdirSync(join(bun, "docs", p.split("/").slice(0, -1).join("/")), { recursive: true });
    writeFileSync(join(bun, "docs", `${p}.mdx`), "---\ntitle: x\n---\n");
  }
  return { dir, bun };
};

const feature = (extra = "") => `
[[feature]]
id = "s3"
bun = "runtime/s3"
names = ["Bun.S3Client"]
cloudflare = ["R2"]
binding = "r2_buckets"
parity = "adapter"
plugin = "shim"
${extra}`;

test("a consistent map passes and the backlog lists the pages nothing covers", async () => {
  const { dir, bun } = tree(`catalog = "docs/research/cloudflare.toml"\n${feature()}`, ["R2"], ["runtime/s3", "runtime/sqlite"]);
  try {
    const r = await validate(dir, bun);
    expect(r.problems).toEqual([]);
    expect(r.parity).toEqual({ adapter: 1 });
    expect(r.uncovered).toEqual(["runtime/sqlite"]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("unknown products, unknown pages, bad parity and duplicates are refused", async () => {
  const bad = feature().replace('cloudflare = ["R2"]', 'cloudflare = ["R3"]').replace('"adapter"', '"magic"');
  const { dir, bun } = tree(`catalog = "docs/research/cloudflare.toml"\n${bad}${feature()}`, ["R2"], ["runtime/sqlite"]);
  try {
    const { problems } = await validate(dir, bun);
    expect(problems.some((p) => p.includes('"R3" is not a product'))).toBe(true);
    expect(problems.some((p) => p.includes('parity "magic"'))).toBe(true);
    expect(problems.some((p) => p.includes("duplicate id"))).toBe(true);
    expect(problems.some((p) => p.includes('"runtime/s3" is not a page'))).toBe(true);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the repository's own map is valid against the catalog and, when present, the Bun checkout", async () => {
  const r = await validate();
  expect(r.problems).toEqual([]);
  expect(r.features).toBeGreaterThan(40);
});
