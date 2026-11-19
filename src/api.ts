import { resolve } from "node:path";
import {
  buildTargetManifest,
  inspectPeWithAbi,
  parseAstWithAbi,
  probeRustAbi,
  readTargetManifest,
  resolveUpstreamAgyPath,
} from "./targets.ts";
import { checkUpgrade, runBenchmarks } from "./upgrade.ts";
import { serviceStatus } from "./daemon.ts";
import { runLearnBridge } from "./mcp_server.ts";
import { pluginStatus } from "./plugin_sync.ts";

export function createFairwindApiHandler(root: string) {
  return (req: Request): Response => {
    const url = new URL(req.url);
    const json = (data: unknown, status = 200) =>
      new Response(JSON.stringify(data, null, 2), {
        status,
        headers: { "content-type": "application/json; charset=utf-8" },
      });

    if (url.pathname === "/health") {
      const abi = probeRustAbi(root);
      return json({ ok: true, product: "fairwind", version: "2.3.0", abiLoaded: abi.loaded, abiVersion: abi.version });
    }
    if (url.pathname === "/status") {
      const abi = probeRustAbi(root);
      const manifest = readTargetManifest(root);
      return json({
        ok: true,
        product: "fairwind",
        root,
        abi,
        manifestReady: Boolean(manifest),
        repoDigest: manifest?.repoDigest ?? null,
        plugin: pluginStatus(root),
        service: serviceStatus(root),
      });
    }
    if (url.pathname === "/targets") {
      const existing = readTargetManifest(root);
      return json({ ok: true, cached: Boolean(existing), manifest: existing ?? buildTargetManifest(root) });
    }
    if (url.pathname === "/inspect") {
      const p = url.searchParams.get("path");
      const target = p ? resolve(root, p) : resolveUpstreamAgyPath();
      if (!target) return json({ ok: false, error: "no PE target found" }, 404);
      const report = inspectPeWithAbi(root, target);
      return json(report, report.ok ? 200 : 400);
    }
    if (url.pathname === "/ast") {
      const lang = url.searchParams.get("lang") ?? "";
      const file = url.searchParams.get("file") ?? "";
      if (!lang || !file) return json({ ok: false, error: "lang and file query params are required" }, 400);
      const res = parseAstWithAbi(root, lang, resolve(root, file));
      return json(res, res.ok ? 200 : 400);
    }
    if (url.pathname === "/upgrade/check") {
      return json(checkUpgrade(root));
    }
    if (url.pathname === "/bench") {
      const iters = Math.max(1, Math.min(10, Number(url.searchParams.get("iterations")) || 3));
      return json(runBenchmarks(root, iters));
    }
    if (url.pathname === "/learn/search") {
      const q = url.searchParams.get("q") ?? "fairwind";
      return json(runLearnBridge("search", q));
    }
    if (url.pathname === "/daemon/status") {
      return json(serviceStatus(root));
    }
    return json(
      {
        ok: false,
        error: "Not found",
        endpoints: [
          "/health",
          "/status",
          "/targets",
          "/inspect?path=...",
          "/ast?lang=rust&file=...",
          "/upgrade/check",
          "/bench?iterations=3",
          "/learn/search?q=fairwind",
          "/daemon/status",
        ],
      },
      404,
    );
  };
}

export function startFairwindApiServer(root: string, host = "127.0.0.1", port = 4379) {
  const server = Bun.serve({
    hostname: host,
    port,
    fetch: createFairwindApiHandler(root),
  });
  return server;
}
