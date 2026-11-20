import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { AyCfManifestSchema } from "../packages/cf-control-plane/src/index.ts";
import {
  buildMissingTunnelIngress,
  classifyHttpResponse,
  collectHttpTargets,
  type HttpTarget,
} from "./cf-http-scan.ts";

const root = resolve(import.meta.dir, "..");

describe("ay-cf http-scan", () => {
  test("collectHttpTargets enumerates declared zones and workers_dev endpoints", () => {
    const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(readFileSync(resolve(root, "packages/cf-control-plane/cf.toml"), "utf8")));
    const targets = collectHttpTargets(manifest);
    expect(targets.some((t) => t.url === "https://aphrody.com/")).toBe(true);
    expect(targets.some((t) => t.url === "https://assets.aphrody.app/" && t.kind === "worker-route" && t.worker === "ay-edge")).toBe(true);
    expect(targets.some((t) => t.url === "https://ay-tools-release.aphrody-labs.workers.dev/latest.json")).toBe(true);
    expect(targets.some((t) => t.url.includes("ssh-prd"))).toBe(false);
  });

  test("classifyHttpResponse distinguishes tunnel catchall 404 and R2 root 404", () => {
    const tunnelTarget: HttpTarget = {
      url: "https://auto.aphrody.com/",
      zone: "aphrody.com",
      hostname: "auto.aphrody.com",
      kind: "tunnel",
      tunnel: "aphrody",
    };
    const r2Target: HttpTarget = {
      url: "https://assets.aphrody.app/",
      zone: "aphrody.app",
      hostname: "assets.aphrody.app",
      kind: "r2-custom-domain",
      bucket: "aphrody-assets",
    };
    expect(classifyHttpResponse(tunnelTarget, 404, "", new Set(["dbfr.aphrody.com"])).classification).toBe(
      "tunnel-catchall-404",
    );
    expect(classifyHttpResponse(r2Target, 404, "", new Set()).classification).toBe("r2-missing-root-index");
    expect(classifyHttpResponse(tunnelTarget, 200, "ok", new Set()).ok).toBe(true);
  });

  test("buildMissingTunnelIngress inserts missing hosts before trailing catch-all 404", () => {
    const cur = [
      { hostname: "search.aphrody.com", service: "http://127.0.0.1:8788" },
      { service: "http_status:404" },
    ];
    const next = buildMissingTunnelIngress(cur, ["auto.aphrody.com", "search.aphrody.com"]);
    expect(next.length).toBe(3);
    expect(next[0]!.hostname).toBe("search.aphrody.com");
    expect(next[1]).toEqual({ hostname: "auto.aphrody.com", service: "http://127.0.0.1:8788" });
    expect(next[2]).toEqual({ service: "http_status:404" });
  });
});
