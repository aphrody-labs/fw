import { describe, expect, test } from "bun:test";
import { createR2Client, mirrorUrlToR2, objectKeyForUrl } from "./r2-cdn-sync.ts";

describe("r2-cdn-sync (Bun.s3)", () => {
  test("objectKeyForUrl normalizes prefix and upstream pathname", () => {
    expect(objectKeyForUrl("https://dxi4wb638ujep.cloudfront.net/1/k/d/y/dykb3jbxeis.png", "zukan/aphrody/")).toBe(
      "zukan/aphrody/1/k/d/y/dykb3jbxeis.png",
    );
  });

  test("createR2Client instantiates Bun.S3Client for Cloudflare R2 endpoint", () => {
    const client = createR2Client({
      bucket: "aphrody-assets",
      accountId: "00000000000000000000000000000000",
      accessKeyId: "test-key",
      secretAccessKey: "test-secret",
    });
    expect(client).toBeDefined();
  });

  test("mirrorUrlToR2 streams upstream bytes and computes sha256 in dry-run", async () => {
    const payload = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
    const res = await mirrorUrlToR2("https://dxi4wb638ujep.cloudfront.net/1/k/d/y/dykb3jbxeis.png", {
      prefix: "zukan",
      dryRun: true,
      fetcher: async () => new Response(payload, { status: 200, headers: { "content-type": "image/png" } }),
    });
    expect(res.uploaded).toBe(false);
    expect(res.bytes).toBe(8);
    expect(res.contentType).toBe("image/png");
    expect(res.key).toBe("zukan/1/k/d/y/dykb3jbxeis.png");
    expect(res.sha256.length).toBe(64);
  });
});
