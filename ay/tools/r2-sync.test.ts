import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { addressedKey, bucketOf, contentTypeOf, createR2Client, mirrorUrlToR2, objectKeyForUrl, sha256Hex, syncAddressed, syncBrand, type Store } from "./r2-sync.ts";

class Memory implements Store {
  readonly objects = new Map<string, { data: Uint8Array; type?: string }>();
  file(key: string) {
    return {
      exists: async () => this.objects.has(key),
      write: async (data: Uint8Array, opts?: { type?: string }) => void this.objects.set(key, { data, type: opts?.type }),
    };
  }
}

describe("r2-sync", () => {
  test("objectKeyForUrl normalizes prefix and upstream pathname", () => {
    expect(objectKeyForUrl("https://dxi4wb638ujep.cloudfront.net/1/k/d/y/dykb3jbxeis.png", "zukan/aphrody/")).toBe("zukan/aphrody/1/k/d/y/dykb3jbxeis.png");
  });

  test("buckets are named by edge.toml, never typed", () => {
    expect(bucketOf("ASSETS")).toBe("aphrody-assets");
    expect(() => bucketOf("NOPE")).toThrow(/no bucket bound as NOPE/);
  });

  test("createR2Client instantiates Bun.S3Client for the R2 endpoint", () => {
    expect(createR2Client({ bucket: "b", accountId: "0".repeat(32), accessKeyId: "k", secretAccessKey: "s" })).toBeDefined();
  });

  test("mirrorUrlToR2 dry run computes the digest without a store", async () => {
    const res = await mirrorUrlToR2("https://cdn.example/a/b.png", { prefix: "zukan", dryRun: true, fetcher: async () => new Response(new Uint8Array([137, 80, 78, 71]), { headers: { "content-type": "image/png" } }) });
    expect(res).toMatchObject({ uploaded: false, bytes: 4, contentType: "image/png", key: "zukan/a/b.png" });
    expect(res.sha256).toHaveLength(64);
  });

  test("a content-addressed key is v1/<kind>/<sha256>.<ext> and unknown kinds are refused", () => {
    const bytes = new TextEncoder().encode("hello");
    expect(addressedKey("icon", bytes, "x.SVG")).toBe(`v1/icon/${sha256Hex(bytes)}.svg`);
    expect(() => addressedKey("nope", bytes, "x.svg")).toThrow(/kind nope/);
    expect(() => addressedKey("icon", bytes, "noext")).toThrow(/extension/);
  });

  test("brand files go to brand/<name> with their type; addressed files are written once", async () => {
    const dir = mkdtempSync(join(tmpdir(), "r2-sync-"));
    writeFileSync(join(dir, "icon.svg"), "<svg/>");
    writeFileSync(join(dir, "manifest.webmanifest"), "{}");
    const mem = new Memory();
    const brand = await syncBrand(dir, { client: mem });
    expect(brand.map((r) => r.key)).toEqual(["brand/icon.svg", "brand/manifest.webmanifest"]);
    expect(mem.objects.get("brand/icon.svg")?.type).toBe("image/svg+xml");
    expect(contentTypeOf("a.webmanifest")).toBe("application/manifest+json");
    const first = await syncAddressed("icon", [join(dir, "icon.svg")], { client: mem });
    const again = await syncAddressed("icon", [join(dir, "icon.svg")], { client: mem });
    expect(first[0]).toMatchObject({ uploaded: true });
    expect(again[0]).toMatchObject({ uploaded: false, unchanged: true });
  });
});
