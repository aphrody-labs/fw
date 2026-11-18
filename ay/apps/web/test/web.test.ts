import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { createWebApp, searchSnapshot } from "../src/app.ts";
import { DEFAULT_PORTAL_SNAPSHOT, loadPortalSnapshot, resolveRepoRoot } from "../src/catalog.ts";
import worker from "../src/worker.ts";

describe("@aphrody/ay-web unified portal & webos", () => {
  const root = resolveRepoRoot();

  test("loadPortalSnapshot reads ay.toml, features.toml, and ay_abi.json", async () => {
    const snap = await loadPortalSnapshot(root);
    expect(snap.product.cli).toBe("ay");
    expect(snap.products.length).toBeGreaterThanOrEqual(5);
    expect(snap.studios.length).toBe(14);
    expect(snap.abi.constantsCount).toBeGreaterThan(0);
    expect(snap.abi.functions.length).toBeGreaterThan(0);
    expect(snap.featureStats.total).toBeGreaterThan(100);
  });

  test("HTML portal, webos, wasm, search, and docs pages return 200 with icons & SEO", async () => {
    const app = createWebApp({ root });
    for (const path of ["/", "/webos", "/wasm", "/search?q=kernel", "/docs"]) {
      const res = await app.fetch(new Request(`http://localhost${path}`));
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/html");
      const text = await res.text();
      expect(text).toContain('rel="icon"');
      expect(text).toContain('name="description"');
      expect(text).toContain("/assets/m3-tokens.css");
    }
  });

  test("brand assets and M3 tokens serve cleanly", async () => {
    const app = createWebApp({ root });
    for (const path of ["/favicon.ico", "/assets/icon.svg", "/apple-touch-icon.png", "/manifest.webmanifest", "/assets/m3-tokens.css"]) {
      const res = await app.fetch(new Request(`http://localhost${path}`));
      expect(res.status).toBe(200);
      const buf = await res.arrayBuffer();
      expect(buf.byteLength).toBeGreaterThan(100);
    }
  });

  test("JSON endpoints preserve full API surface cleanly without leaked host IPs", async () => {
    const app = createWebApp({ root });
    for (const ep of ["/healthz", "/api/status", "/api/orgs", "/api/fleet", "/api/registries", "/api/arena", "/api/abi", "/api/quota", "/api/search?q=os"]) {
      const res = await app.fetch(new Request(`http://localhost${ep}`));
      expect(res.status).toBe(200);
      const body = await res.text();
      expect(body).not.toContain("72.60.");
      expect(body).not.toContain("51.77.");
      expect(body).not.toContain("10.200.77.");
    }
  });

  test("POST /api/nysh executes virtual sandbox pipeline", async () => {
    const app = createWebApp({ root });
    const res = await app.fetch(
      new Request("http://localhost/api/nysh", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ cmd: "cat /ay/manifest.json | grep aphrody" }),
      }),
    );
    expect(res.status).toBe(200);
    const body = (await res.json()) as { exitCode: number; stdout: string };
    expect(body.exitCode).toBe(0);
    expect(body.stdout).toContain("aphrody");
  });

  test("Accept: text/markdown on /docs returns agent markdown", async () => {
    const app = createWebApp({ root });
    const res = await app.fetch(new Request("http://localhost/docs", { headers: { accept: "text/markdown" } }));
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("text/markdown");
    expect(await res.text()).toContain("# Aphrody Documentation");
  });

  test("dash mount forwards /dash/* without duplicating chromium-dash code", async () => {
    let forwardedUrl = "";
    const app = createWebApp({
      root,
      dashFetch: async (r) => {
        forwardedUrl = r.url;
        return Response.json({ mounted: true });
      },
    });
    const res = await app.fetch(new Request("http://localhost/dash/releases?platform=Linux"));
    expect(res.status).toBe(200);
    expect(forwardedUrl).toBe("http://localhost/releases?platform=Linux");
  });

  test("stateless edge worker handles studio hostnames and search", async () => {
    const res = await worker.fetch(new Request("https://filmcraft.aphrody.app/"), {});
    expect(res.status).toBe(200);
    expect(await res.text()).toContain("FilmCraft");
    const hits = searchSnapshot(DEFAULT_PORTAL_SNAPSHOT, "browser");
    expect(hits.some((h) => h.id === "browser")).toBe(true);
  });

  test("target/corelib wasm executes when compiled corelib.wasm exists", async () => {
    const prebuilt = Bun.file(join(root, "target/corelib/wasm-base/corelib.wasm"));
    if (await prebuilt.exists()) {
      const app = createWebApp({ root });
      const res = await app.fetch(new Request("http://localhost/api/wasm/run"));
      expect(res.status).toBe(200);
      const data = (await res.json()) as { available: boolean; boot: number; abiVersion: number };
      expect(data.available).toBe(true);
      expect(data.boot).toBe(0);
      expect(data.abiVersion).toBeGreaterThanOrEqual(1);
    }
  });
});
