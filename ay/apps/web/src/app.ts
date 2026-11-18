import { join } from "node:path";
import { CLOUDFLARE_OS_GATEKEEPERS, evalVirtualNyshPipeline } from "@aphrody/ay-edge-stack";
import { DEFAULT_QUOTA_POLICY, evaluateQuotaRouting, type QuotaEvaluation } from "../../../tools/cf-quota.ts";
import { instantiateCore } from "../../../corelib/host/host.ts";
import {
  DEFAULT_PORTAL_SNAPSHOT,
  STUDIO_HOST_MAP,
  loadPortalSnapshot,
  resolveRepoRoot,
  type PortalSnapshot,
} from "./catalog.ts";
import {
  renderDocsMarkdown,
  renderDocsPage,
  renderPortalHome,
  renderSearchPage,
  renderWasmPage,
  renderWebOsPage,
} from "./ui.ts";

export interface WebAppOptions {
  root?: string;
  snapshot?: PortalSnapshot;
  searchOrigin?: string;
  dashFetch?: (req: Request) => Promise<Response>;
  wasmBytes?: Uint8Array;
  quotaEvaluator?: () => QuotaEvaluation;
}

const ASSETS: Readonly<Record<string, [string, string]>> = {
  "/favicon.ico": ["brand/assets/favicon.ico", "image/x-icon"],
  "/apple-touch-icon.png": ["brand/assets/apple-touch-icon.png", "image/png"],
  "/manifest.webmanifest": ["brand/assets/manifest.webmanifest", "application/manifest+json; charset=utf-8"],
  "/assets/m3-tokens.css": ["brand/tokens/m3-tokens.css", "text/css; charset=utf-8"],
  "/assets/icon.svg": ["brand/assets/icon.svg", "image/svg+xml"],
  "/assets/icon-192.png": ["brand/assets/icon-192.png", "image/png"],
  "/assets/icon-512.png": ["brand/assets/icon-512.png", "image/png"],
  "/assets/maskable-192.png": ["brand/assets/maskable-192.png", "image/png"],
  "/assets/maskable-512.png": ["brand/assets/maskable-512.png", "image/png"],
};

const PROTOCOLS = ["OpenAI-compat", "OpenAPI", "MCP", "WebMCP", "ACP", "A2A"] as const;

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data, null, 2), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "public, max-age=30",
      "x-content-type-options": "nosniff",
    },
  });
}

function html(body: string, status = 200): Response {
  return new Response(body, {
    status,
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-cache",
      "x-content-type-options": "nosniff",
    },
  });
}

export function searchSnapshot(
  snapshot: PortalSnapshot,
  rawQuery: string,
  limit = 40,
): Array<{ kind: string; id: string; title: string; detail: string }> {
  const q = rawQuery.trim().toLowerCase();
  const hits: Array<{ kind: string; id: string; title: string; detail: string }> = [];
  const matches = (text: string) => !q || text.toLowerCase().includes(q);

  for (const p of snapshot.products) {
    const line = `${p.key} ${p.title} ${p.owner} ${p.keywords.join(" ")}`;
    if (matches(line)) {
      hits.push({ kind: "product", id: p.key, title: p.title, detail: `Owner: ${p.owner} — ${p.keywords.slice(0, 6).join(", ")}` });
    }
  }
  for (const s of snapshot.studios) {
    const line = `${s.id} ${s.title} ${s.host} ${s.summary}`;
    if (matches(line)) {
      hits.push({ kind: "studio", id: s.id, title: s.title, detail: `${s.host} — ${s.summary}` });
    }
  }
  for (const f of snapshot.features) {
    const line = `${f.product} ${f.id} ${f.title} ${f.layer} ${f.status}`;
    if (matches(line)) {
      hits.push({ kind: "feature", id: `${f.product}/${f.id}`, title: f.title, detail: `${f.layer} · ${f.status}` });
    }
    if (hits.length >= limit) break;
  }
  if (hits.length < limit) {
    for (const fn of snapshot.abi.functions) {
      if (matches(`${fn.name} ${fn.doc ?? ""}`)) {
        hits.push({ kind: "abi", id: fn.name, title: `${fn.name}() -> ${fn.returns}`, detail: fn.doc ?? "C23 Corelib ABI export" });
      }
      if (hits.length >= limit) break;
    }
  }
  return hits.slice(0, limit);
}

function defaultQuotaEvaluation(): QuotaEvaluation {
  return evaluateQuotaRouting({
    cloudflare: {
      tier: "paid",
      workersCount: CLOUDFLARE_OS_GATEKEEPERS.length,
      dailyWorkerRequests: 0,
      r2StorageBytes: 0,
      r2ClassAMonthly: 0,
      r2ClassBMonthly: 0,
    },
    githubActions: { monthlyMinutesUsed: 0, repos: [] },
    vpsFleet: [
      { role: "prod", reachable: true, diskUsedPct: 45, load1m: 0.1, cpus: 4 },
      { role: "build", reachable: true, diskUsedPct: 40, load1m: 0.5, cpus: 8 },
      { role: "test", reachable: true, diskUsedPct: 35, load1m: 0.2, cpus: 4 },
    ],
  });
}

function buildVfs(snapshot: PortalSnapshot): Map<string, string> {
  return new Map<string, string>([
    ["/ay/manifest.json", JSON.stringify({ product: snapshot.product, products: snapshot.products.map((p) => p.key) }, null, 2)],
    ["/ay/abi.json", JSON.stringify({ version: snapshot.abi.version, constants: snapshot.abi.constantsCount }, null, 2)],
    ["/ay/studios.txt", snapshot.studios.map((s) => `${s.id}\t${s.host}\t${s.title}`).join("\n") + "\n"],
  ]);
}

export function createWebApp(options: WebAppOptions = {}) {
  const root = options.root ?? resolveRepoRoot();
  let cachedSnapshot: PortalSnapshot | undefined = options.snapshot;

  async function getSnapshot(): Promise<PortalSnapshot> {
    if (cachedSnapshot) return cachedSnapshot;
    try {
      cachedSnapshot = await loadPortalSnapshot(root);
    } catch {
      cachedSnapshot = DEFAULT_PORTAL_SNAPSHOT;
    }
    return cachedSnapshot;
  }

  async function getWasmBytes(): Promise<Uint8Array | undefined> {
    if (options.wasmBytes) return options.wasmBytes;
    const wasmFile = Bun.file(join(root, "target/corelib/wasm-base/corelib.wasm"));
    if (await wasmFile.exists()) {
      return new Uint8Array(await wasmFile.arrayBuffer());
    }
    return undefined;
  }

  async function serveAsset(pathname: string): Promise<Response | undefined> {
    const hit = ASSETS[pathname];
    if (!hit) return undefined;
    const file = Bun.file(join(root, hit[0]));
    if (!(await file.exists())) return undefined;
    return new Response(file, {
      headers: { "content-type": hit[1], "cache-control": "public, max-age=3600" },
    });
  }

  return {
    async fetch(req: Request): Promise<Response> {
      const u = new URL(req.url);
      const path = u.pathname.replace(/\/+$/, "") || "/";
      const origin = u.origin;

      if (path === "/health" || path === "/healthz" || path === "/api/health") {
        const snapshot = await getSnapshot();
        return json({
          ok: true,
          service: "ay-web",
          product: snapshot.product.name,
          cli: snapshot.product.cli,
          gatekeepers: CLOUDFLARE_OS_GATEKEEPERS.length,
        });
      }

      if (req.method === "GET" || req.method === "HEAD") {
        const asset = await serveAsset(u.pathname);
        if (asset) return asset;
      }

      if (options.dashFetch && (u.pathname === "/dash" || u.pathname.startsWith("/dash/") || u.pathname.startsWith("/fetch_") || u.pathname.startsWith("/api/v0/") || u.pathname.startsWith("/cros/"))) {
        const rewrittenUrl = new URL(req.url);
        if (u.pathname === "/dash" || u.pathname.startsWith("/dash/")) {
          rewrittenUrl.pathname = u.pathname.replace(/^\/dash/, "") || "/";
        }
        return options.dashFetch(new Request(rewrittenUrl, req));
      }

      const snapshot = await getSnapshot();

      if (path === "/robots.txt") {
        return new Response(`User-agent: *\nAllow: /\nSitemap: ${origin}/sitemap.xml\n`, {
          headers: { "content-type": "text/plain; charset=utf-8" },
        });
      }
      if (path === "/sitemap.xml") {
        const urls = ["/", "/webos", "/search", "/wasm", "/docs", "/dash/"];
        const body = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls.map((p) => `  <url><loc>${origin}${p}</loc></url>`).join("\n")}\n</urlset>\n`;
        return new Response(body, { headers: { "content-type": "application/xml; charset=utf-8" } });
      }
      if (path === "/llms.txt") {
        return new Response(renderDocsMarkdown(snapshot), { headers: { "content-type": "text/markdown; charset=utf-8" } });
      }

      if (path === "/api/status") {
        return json({
          system: `${snapshot.product.display} Unified Platform`,
          cli: snapshot.product.cli,
          orgs: snapshot.orgs,
          products: snapshot.products.map((p) => ({
            key: p.key,
            title: p.title,
            owner: p.owner,
            features: snapshot.featureStats.byProduct[p.key] ?? { total: 0, done: 0 },
          })),
          featureStats: snapshot.featureStats,
          sdk: snapshot.sdk,
          studiosCount: snapshot.studios.length,
        });
      }
      if (path === "/api/orgs") {
        return json({ orgs: snapshot.orgs, repoCount: snapshot.repos.length, repos: snapshot.repos });
      }
      if (path === "/api/fleet") {
        return json({
          roles: snapshot.fleetRoles,
          gatekeepers: CLOUDFLARE_OS_GATEKEEPERS.map((g) => ({ id: g.id, workerName: g.workerName, scopes: g.allowedScopes })),
          r2Buckets: snapshot.r2Buckets,
        });
      }
      if (path === "/api/registries") {
        return json({
          npm: { scope: snapshot.registries.npm, legacyCompat: snapshot.registries.legacyNpm },
          crates: { ownPrefix: snapshot.registries.ownCrates, legacyPrefix: snapshot.registries.legacyCratesPrefix, legacyCompatCount: snapshot.registries.legacyCratesCount },
        });
      }
      if (path === "/api/arena") {
        return json({ protocols: PROTOCOLS, studios: snapshot.studios.length });
      }
      if (path === "/api/abi") {
        return json(snapshot.abi);
      }
      if (path === "/api/quota") {
        const evaluation = options.quotaEvaluator ? options.quotaEvaluator() : defaultQuotaEvaluation();
        return json({ policy: DEFAULT_QUOTA_POLICY, evaluation });
      }

      if (path === "/api/search") {
        const q = u.searchParams.get("q") ?? "";
        if (options.searchOrigin) {
          try {
            const upstream = await fetch(`${options.searchOrigin.replace(/\/+$/, "")}/api/search?q=${encodeURIComponent(q)}`);
            if (upstream.ok) return upstream;
          } catch {
            // Fall back to local deterministic platform search when origin is unreachable.
          }
        }
        const hits = searchSnapshot(snapshot, q);
        return json({ query: q, count: hits.length, results: hits });
      }

      if (path === "/api/nysh" && req.method === "POST") {
        const payload = (await req.json().catch(() => ({}))) as { cmd?: unknown };
        const cmd = typeof payload.cmd === "string" ? payload.cmd.trim() : "";
        if (!cmd) return json({ error: "missing cmd string", exitCode: 2 }, 400);
        const result = evalVirtualNyshPipeline(cmd, buildVfs(snapshot));
        return json(result, result.exitCode === 0 ? 200 : 400);
      }

      if (path === "/wasm/corelib.wasm") {
        const bytes = await getWasmBytes();
        if (!bytes) return json({ error: "corelib.wasm not built; run `ay wasm` or build target/corelib/wasm-base/corelib.wasm" }, 404);
        return new Response(new Uint8Array(bytes), { headers: { "content-type": "application/wasm", "cache-control": "public, max-age=300" } });
      }

      if (path === "/api/wasm/run") {
        const bytes = await getWasmBytes();
        if (!bytes) {
          return json({
            available: false,
            abiVersion: snapshot.abi.version,
            constantsCount: snapshot.abi.constantsCount,
            hint: "Run `ay wasm` locally to compile target/corelib/wasm-base/corelib.wasm.",
          });
        }
        const logs: number[] = [];
        const core = instantiateCore(bytes, (b) => logs.push(b));
        const boot = core.ay_boot();
        const abiVersion = core.ay_abi_version();
        const argCountSysLog = core.ay_arg_count(2);
        const allowMatch = core.ay_rights_allow(7, 3);
        const allowDeny = core.ay_rights_allow(1, 3);
        return json({
          available: true,
          boot,
          abiVersion,
          argCountSysLog,
          rightsCheck: { allowMatch, allowDeny },
          serialOutput: new TextDecoder().decode(new Uint8Array(logs)),
        });
      }

      const studioParam = u.searchParams.get("studio") ?? STUDIO_HOST_MAP[u.hostname.toLowerCase()];
      const activeStudio = studioParam ? snapshot.studios.find((s) => s.id === studioParam) : undefined;
      if (activeStudio && path === "/") {
        return html(renderWebOsPage(snapshot, origin, activeStudio));
      }
      if (path === "/webos" || path === "/desktop") {
        return html(renderWebOsPage(snapshot, origin, activeStudio));
      }
      if (path === "/wasm") {
        return html(renderWasmPage(snapshot, origin, Boolean(await getWasmBytes())));
      }
      if (path === "/search") {
        const q = u.searchParams.get("q") ?? "";
        const hits = searchSnapshot(snapshot, q);
        return html(renderSearchPage(snapshot, origin, q, hits));
      }
      if (path === "/docs") {
        const accept = req.headers.get("accept") ?? "";
        if (accept.includes("text/markdown")) {
          return new Response(renderDocsMarkdown(snapshot), { headers: { "content-type": "text/markdown; charset=utf-8" } });
        }
        return html(renderDocsPage(snapshot, origin));
      }
      if (path === "/") {
        return html(renderPortalHome(snapshot, origin));
      }

      return json({ error: `not found: ${path}` }, 404);
    },
  };
}
