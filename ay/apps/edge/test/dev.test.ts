// The unified Worker in a real workerd: `wrangler dev --local` with the generated config, the service-mode scripts replaced by stubs
// (multi-worker dev), R2 filled with `wrangler r2 object put --local` from a release produced by tools/release-publish.ts, then real
// HTTP requests per route with the Host of that route. Skipped only when wrangler or workerd cannot start here (the reason is printed).
import { afterAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { root } from "../../../tools/corelib.ts";
import { enabledServices, loadEdgeConfig, renderWrangler } from "../../../tools/edge-config.ts";
import { build } from "../../../tools/edge-wasm.ts";

const cfg = loadEdgeConfig();
const edge = join(root, "apps/edge");
const work = mkdtempSync(join(tmpdir(), "ay-edge-dev-"));
const persist = join(work, "state");
const wrangler = Bun.which("wrangler") ?? Bun.which("bunx") ?? "bunx";
const wranglerArgs = Bun.which("wrangler") ? [] : ["wrangler"];
const env = { ...process.env, WRANGLER_SEND_METRICS: "false" };
const domains = cfg.domains as Record<string, string>;
const rootDomain = domains.root!;

/** The corelib wasm needs a clang with --no-wasm-opt (LLVM 19+): without it the Worker cannot be bundled and the workerd tests skip. */
let toolchain = "";
try {
  build();
} catch (e) {
  toolchain = `wasm toolchain unavailable: ${String(e).split("\n")[0]}`;
}

const run = (cmd: string[]) => Bun.spawnSync(cmd, { cwd: edge, stdout: "pipe", stderr: "pipe", env });

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((n) => (statSync(join(dir, n)).isDirectory() ? walk(join(dir, n)) : [join(dir, n)]));
}

/** A release of the product `ay` laid out by the publisher itself, then put into the local R2 of the releases bucket. */
function seed(): void {
  const art = join(work, "artifacts");
  const layout = join(work, "layout");
  mkdirSync(art, { recursive: true });
  for (const [name, body] of [["ay-linux-x86_64", "linux-binary"], ["ay-linux-aarch64", "arm-binary"], ["ay-windows-x86_64.exe", "win-binary"]]) writeFileSync(join(art, name!), body!);
  const p = Bun.spawnSync(
    ["bun", join(root, "tools/release-publish.ts"), "publish", "--product", "ay", "--version", "9.9.9", "--channel", "stable", "--tag", "v9.9.9", "--commit", "0123abc", "--dir", art, "--out", layout],
    { cwd: root, stdout: "pipe", stderr: "pipe" },
  );
  if (p.exitCode !== 0) throw new Error(`release-publish failed: ${p.stderr.toString()}${p.stdout.toString()}`);
  const puts: Array<[string, string, string]> = walk(layout).map((f) => [cfg.buckets.RELEASES!, relative(layout, f).replaceAll("\\", "/"), f]);
  const icon = join(work, "icon.svg");
  writeFileSync(icon, "<svg xmlns='http://www.w3.org/2000/svg'/>");
  puts.push([cfg.buckets.ASSETS!, "brand/icon.svg", icon]);
  puts.push([cfg.buckets.ASSETS!, "notes/readme.txt", icon]);
  const tools = join(work, "latest.json");
  writeFileSync(tools, '{"version":"tools-1"}');
  puts.push([cfg.buckets.CHROMIUM!, "ay-tools/latest.json", tools]);
  for (const [bucket, key, file] of puts) {
    const r = run([wrangler, ...wranglerArgs, "r2", "object", "put", `${bucket}/${key}`, "--file", file, "--local", "--persist-to", persist, "-c", "wrangler.jsonc"]);
    if (r.exitCode !== 0) throw new Error(`r2 put ${bucket}/${key}: ${r.stderr.toString().slice(-300)}`);
  }
}

async function freePort(): Promise<number> {
  const s = Bun.serve({ port: 0, fetch: () => new Response("") });
  const port = s.port!;
  await s.stop(true);
  return port;
}

async function start(): Promise<{ base: string; stop: () => void } | string> {
  seed();
  const scripts = enabledServices(cfg).filter(([, s]) => s.mode === "service");
  const configs = ["wrangler.jsonc"];
  const stubs: string[] = [];
  for (const [, s] of scripts) {
    const file = join(work, `stub-${s.script}.jsonc`);
    writeFileSync(file, JSON.stringify({ name: s.script, main: join(edge, "test/stub.ts"), compatibility_date: "2026-11-18", vars: { NAME: s.script } }));
    stubs.push(file);
  }
  const rendered = join(work, "edge.jsonc");
  const w = renderWrangler(cfg) as Record<string, unknown>;
  w.main = join(edge, "src/index.ts");
  delete w.$schema;
  (w.vars as Record<string, unknown>).EDGE_HOST_HEADER = "x-edge-host";
  writeFileSync(rendered, JSON.stringify(w));
  const port = await freePort();
  const args = ["dev", "-c", rendered, ...stubs.flatMap((s) => ["-c", s]), "--local", "--port", String(port), "--ip", "127.0.0.1", "--persist-to", persist];
  void configs;
  const proc = Bun.spawn([wrangler, ...wranglerArgs, ...args], { cwd: edge, stdout: "pipe", stderr: "pipe", env });
  let log = "";
  const pump = async (s: ReadableStream<Uint8Array>) => {
    for await (const chunk of s) log += new TextDecoder().decode(chunk);
  };
  void pump(proc.stdout);
  void pump(proc.stderr);
  const stop = () => {
    if (process.platform === "win32") Bun.spawnSync(["taskkill", "/PID", String(proc.pid), "/T", "/F"], { stdout: "ignore", stderr: "ignore" });
    else proc.kill();
  };
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 160; i++) {
    if (proc.exitCode !== null) return `wrangler dev exited ${proc.exitCode}: ${log.slice(-600)}`;
    try {
      if ((await fetch(`${base}/_/`)).ok) return { base, stop };
    } catch {}
    await Bun.sleep(500);
  }
  stop();
  return `wrangler dev did not become ready: ${log.slice(-600)}`;
}

const server = toolchain || (await start());
const running = typeof server === "object";
if (!running) console.warn(`skipping the workerd tests: ${server}`);
afterAll(() => {
  if (running) server.stop();
});

describe.skipIf(!running)("unified Worker in workerd", () => {
  const base = () => (server as { base: string }).base;
  /** A request addressed to a host of the table: the Host header carries the route, the socket stays local. */
  const get = (host: string, path: string, init: RequestInit = {}) => fetch(`${base()}${path}`, { ...init, headers: { ...(init.headers as Record<string, string>), "x-edge-host": host } });
  const preview = (service: string, path: string) => fetch(`${base()}/_/${service}${path}`, { headers: { "x-edge-host": "ay-edge.example.workers.dev" } });

  test("the service list is the one rendered from edge.toml", async () => {
    const body = (await (await fetch(`${base()}/_/`)).json()) as { services: Record<string, unknown>; routes: unknown[] };
    expect(Object.keys(body.services).sort()).toEqual(enabledServices(cfg).map(([n]) => n).sort());
  });

  test("releases: manifest, per-platform redirect, ranged asset, checksums", async () => {
    const manifest = (await (await get(`releases.${rootDomain}`, "/manifest.json")).json()) as { products: { ay: { stable: { version: string } } } };
    expect(manifest.products.ay.stable.version).toBe("9.9.9");
    const redirect = await get(`downloads.${rootDomain}`, "/ay/latest/windows-x86_64", { redirect: "manual" });
    expect(redirect.status).toBe(302);
    expect(redirect.headers.get("location")).toContain("/ay/9.9.9/ay-windows-x86_64.exe");
    const part = await get(`releases.${rootDomain}`, "/ay/9.9.9/ay-linux-x86_64", { headers: { range: "bytes=0-4" } });
    expect(part.status).toBe(206);
    expect(await part.text()).toBe("linux");
    const sums = await (await get(`releases.${rootDomain}`, "/ay/9.9.9/SHA256SUMS")).text();
    expect(sums).toMatch(/^[0-9a-f]{64} {2}ay-linux-x86_64$/m);
    const latest = (await (await get(`releases.${rootDomain}`, "/api/releases/ay/stable/latest")).json()) as { assets: Array<{ name: string; url: string }> };
    expect(latest.assets.map((a) => a.name).sort()).toEqual(["ay-linux-aarch64", "ay-linux-x86_64", "ay-windows-x86_64.exe"]);
  });

  test("releases: the apex path /download is the portal, the rest of the apex is not routed", async () => {
    const portal = await get(rootDomain, "/download", { headers: { "user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" } });
    expect(portal.headers.get("content-type")).toContain("text/html");
    expect(await portal.text()).toContain("ay-windows-x86_64.exe");
    expect((await get(rootDomain, "/pricing")).status).toBe(404);
  });

  test("releases: brand files from the assets bucket, ay-tools from the chromium bucket", async () => {
    expect((await get(`releases.${rootDomain}`, "/brand/icon.svg")).headers.get("content-type")).toContain("image/svg+xml");
    expect(((await (await get(`releases.${rootDomain}`, "/ay-tools/latest.json")).json()) as { version: string }).version).toBe("tools-1");
  });

  test("assets: plain keys under /r2-assets and the gateway on its host", async () => {
    expect(await (await get(rootDomain, "/r2-assets/notes/readme.txt")).text()).toContain("svg");
    expect((await get(rootDomain, "/r2-assets/../x")).status).toBe(404);
    expect(((await (await get(`assets.${rootDomain}`, "/healthz")).json()) as { service: string }).service).toBe("ay-cf-gateway");
    expect((await get(`assets.${rootDomain}`, `/v1/icon/${"0".repeat(64)}.svg`)).status).toBe(404);
    expect((await get(`assets.${rootDomain}`, "/v1/icon/not-a-hash")).status).toBe(400);
  });

  test("service mode: every script gets the request it owns", async () => {
    const via = async (host: string, path: string) => (await (await get(host, path)).json()) as { stub: string; host: string; path: string };
    expect(await via(`docs.${rootDomain}`, "/guide")).toMatchObject({ stub: "aphrody-worker", host: `docs.${rootDomain}`, path: "/guide" });
    expect((await via(rootDomain, "/github")).stub).toBe("aphrody-worker");
    expect((await via(`learn.${rootDomain}`, "/api/learn")).stub).toBe("aphrody-learn");
    expect((await via(`auto.${rootDomain}`, "/api/x")).stub).toBe("auto");
    expect((await via(`cdn.${domains.shenron}`, "/img/a.png")).stub).toBe("shenron-cdn");
  });

  test("preview prefix reaches a service on an unrouted host, with links that stay inside the preview", async () => {
    expect(((await (await preview("aphrody", "/anything")).json()) as { stub: string; path: string })).toMatchObject({ stub: "aphrody-worker", path: "/anything" });
    const latest = (await (await preview("releases", "/api/releases/ay/stable/latest")).json()) as { assets: Array<{ url: string }> };
    expect(latest.assets[0]!.url).toContain("/_/releases/ay/9.9.9/");
  });

  test("unrouted requests are 404 JSON, never a service", async () => {
    const res = await get("unknown.example", "/x");
    expect(res.status).toBe(404);
    expect(await res.json()).toMatchObject({ error: "no route" });
  });
});
