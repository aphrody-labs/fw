#!/usr/bin/env bun
// Headless engines against Chrome headless on the same pages, over raw CDP.
//   bun compare.ts --chrome /usr/bin/google-chrome --lite ./obscura --pages ./pages [--runs 5] [--out report.json]
// Measures: startup ms (spawn to /json/version), page-load ms (Page.navigate to load), process-tree RSS after load,
// WebAssembly throughput (pages/wasm-bench.html), WebGPU pixel readback (pages/webgpu-readback.html),
// the CDP command list below, and binary bytes. Linux only (reads /proc through ps).
import { spawn, type Subprocess } from "bun";
import { mkdtempSync, rmSync, statSync, writeFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const argv = process.argv.slice(2);
const opt = (name: string, fallback?: string) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? argv[i + 1] : fallback;
};
const chromePath = opt("chrome", "/usr/bin/google-chrome")!;
const litePath = opt("lite");
const pagesDir = opt("pages", "./pages")!;
const runs = Number(opt("runs", "5"));
const outPath = opt("out");
const sitePages = (opt("site", "") ?? "").split(",").filter(Boolean);
const liteArgs = (opt("lite-args", "serve --allow-private-network --port {port}") ?? "").split(" ");
const chromeExtra = (opt("chrome-args", "--enable-unsafe-webgpu --enable-features=Vulkan --use-angle=vulkan --disable-vulkan-surface --no-sandbox") ?? "").split(" ").filter(Boolean);

type Engine = { name: string; path: string; launch: (port: number, dir: string) => string[] };
const engines: Engine[] = [
  {
    name: "chrome",
    path: chromePath,
    launch: (port, dir) => [chromePath, "--headless=new", "--no-first-run", `--remote-debugging-port=${port}`, `--user-data-dir=${dir}`, ...chromeExtra, "about:blank"],
  },
];
if (litePath) engines.push({ name: "lite", path: litePath, launch: (port) => [litePath, ...liteArgs.map((a) => a.replace("{port}", String(port)))] });

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const median = (xs: number[]) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length ? s[Math.floor(s.length / 2)] : NaN;
};

const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch(req) {
    const url = new URL(req.url);
    const name = url.pathname.replace(/^\/+/, "") || "index.html";
    const file = Bun.file(join(pagesDir, name));
    return file.size ? new Response(file) : new Response("not found", { status: 404 });
  },
});
const origin = `http://127.0.0.1:${server.port}`;

function freePort() {
  const s = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: () => new Response("") });
  const p = s.port;
  s.stop(true);
  return p;
}

function treeRssKb(root: number): number {
  const ps = Bun.spawnSync(["ps", "-eo", "pid=,ppid=,rss="]).stdout.toString().trim().split("\n");
  const rows = ps.map((l) => l.trim().split(/\s+/).map(Number));
  const kids = new Map<number, number[]>();
  for (const [pid, ppid] of rows) kids.set(ppid, [...(kids.get(ppid) ?? []), pid]);
  const rss = new Map(rows.map(([pid, , r]) => [pid, r]));
  let total = 0;
  const stack = [root];
  while (stack.length) {
    const pid = stack.pop()!;
    total += rss.get(pid) ?? 0;
    stack.push(...(kids.get(pid) ?? []));
  }
  return total;
}

class Cdp {
  ws!: WebSocket;
  next = 1;
  pending = new Map<number, (m: any) => void>();
  listeners: ((m: any) => void)[] = [];
  static async connect(url: string) {
    const c = new Cdp();
    c.ws = new WebSocket(url);
    await new Promise<void>((res, rej) => {
      c.ws.onopen = () => res();
      c.ws.onerror = () => rej(new Error("ws error"));
    });
    c.ws.onmessage = (ev) => {
      const m = JSON.parse(String(ev.data));
      if (m.id && c.pending.has(m.id)) {
        c.pending.get(m.id)!(m);
        c.pending.delete(m.id);
      } else for (const l of c.listeners) l(m);
    };
    return c;
  }
  send(method: string, params: object = {}, sessionId?: string, timeoutMs = 20000): Promise<any> {
    const id = this.next++;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        resolve({ error: { code: -1, message: `timeout ${method}` } });
      }, timeoutMs);
      this.pending.set(id, (m) => {
        clearTimeout(timer);
        resolve(m);
      });
      this.ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }
  waitEvent(method: string, sessionId: string, timeoutMs = 20000) {
    return new Promise<any>((resolve) => {
      const timer = setTimeout(() => resolve(null), timeoutMs);
      const l = (m: any) => {
        if (m.method === method && m.sessionId === sessionId) {
          clearTimeout(timer);
          this.listeners = this.listeners.filter((x) => x !== l);
          resolve(m);
        }
      };
      this.listeners.push(l);
    });
  }
  close() {
    this.ws.close();
  }
}

async function startEngine(e: Engine) {
  const port = freePort();
  const dir = mkdtempSync(join(tmpdir(), "fwb-"));
  const t0 = performance.now();
  const proc = spawn(e.launch(port, dir), { stdout: "ignore", stderr: "ignore", stdin: "ignore" });
  let version: any = null;
  for (let i = 0; i < 600 && !version; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (r.ok) version = await r.json();
    } catch {}
    if (!version) await sleep(5);
  }
  const startMs = performance.now() - t0;
  return { proc, port, dir, version, startMs };
}

async function stopEngine(s: { proc: Subprocess; dir: string }) {
  s.proc.kill();
  await s.proc.exited;
  rmSync(s.dir, { recursive: true, force: true });
}

async function openPage(cdp: Cdp) {
  const t = await cdp.send("Target.createTarget", { url: "about:blank" });
  const targetId = t.result?.targetId;
  const a = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  return { targetId, sessionId: a.result?.sessionId as string | undefined, created: t, attached: a };
}

async function loadPage(cdp: Cdp, sessionId: string, url: string, waitResult: boolean) {
  await cdp.send("Page.enable", {}, sessionId);
  const loaded = cdp.waitEvent("Page.loadEventFired", sessionId, 30000);
  const t0 = performance.now();
  const nav = await cdp.send("Page.navigate", { url }, sessionId);
  const ev = await loaded;
  let loadMs = performance.now() - t0;
  if (!ev) {
    // engines that skip the event: poll readyState
    for (let i = 0; i < 2000; i++) {
      const r = await cdp.send("Runtime.evaluate", { expression: "document.readyState", returnByValue: true }, sessionId);
      if (r.result?.result?.value === "complete") break;
      await sleep(5);
    }
    loadMs = performance.now() - t0;
  }
  let result: any = null;
  if (waitResult) {
    for (let i = 0; i < 600 && result == null; i++) {
      const r = await cdp.send("Runtime.evaluate", { expression: "JSON.stringify(window.__result ?? null)", returnByValue: true }, sessionId);
      const v = r.result?.result?.value;
      if (v && v !== "null") result = JSON.parse(v);
      else await sleep(25);
    }
  }
  return { loadMs, nav, result };
}

const CDP_COMMANDS: { method: string; params?: (ctx: any) => object; scope: "browser" | "page" }[] = [
  { method: "Browser.getVersion", scope: "browser" },
  { method: "Target.getTargets", scope: "browser" },
  { method: "Target.createTarget", params: () => ({ url: "about:blank" }), scope: "browser" },
  { method: "Page.enable", scope: "page" },
  { method: "Page.navigate", params: (c) => ({ url: `${origin}/cdp.html` }), scope: "page" },
  { method: "Page.getFrameTree", scope: "page" },
  { method: "Page.addScriptToEvaluateOnNewDocument", params: () => ({ source: "window.__x = 1" }), scope: "page" },
  { method: "Page.captureScreenshot", params: () => ({ format: "png" }), scope: "page" },
  { method: "Page.printToPDF", scope: "page" },
  { method: "Page.reload", scope: "page" },
  { method: "Runtime.enable", scope: "page" },
  { method: "Runtime.evaluate", params: () => ({ expression: "1+1", returnByValue: true }), scope: "page" },
  { method: "Runtime.callFunctionOn", params: (c) => ({ functionDeclaration: "function(){return 7}", objectId: c.docObjectId, returnByValue: true }), scope: "page" },
  { method: "Runtime.getProperties", params: (c) => ({ objectId: c.docObjectId }), scope: "page" },
  { method: "DOM.enable", scope: "page" },
  { method: "DOM.getDocument", params: () => ({ depth: 1 }), scope: "page" },
  { method: "DOM.querySelector", params: (c) => ({ nodeId: c.rootNodeId, selector: "h1" }), scope: "page" },
  { method: "DOM.getOuterHTML", params: (c) => ({ nodeId: c.h1NodeId }), scope: "page" },
  { method: "DOM.resolveNode", params: (c) => ({ nodeId: c.h1NodeId }), scope: "page" },
  { method: "Network.enable", scope: "page" },
  { method: "Network.setExtraHTTPHeaders", params: () => ({ headers: { "x-fw": "1" } }), scope: "page" },
  { method: "Network.setCookie", params: () => ({ name: "a", value: "b", url: `${origin}/` }), scope: "page" },
  { method: "Network.getCookies", params: () => ({ urls: [`${origin}/`] }), scope: "page" },
  { method: "Emulation.setDeviceMetricsOverride", params: () => ({ width: 800, height: 600, deviceScaleFactor: 1, mobile: false }), scope: "page" },
  { method: "Emulation.setUserAgentOverride", params: () => ({ userAgent: "fw-test" }), scope: "page" },
  { method: "Input.dispatchMouseEvent", params: () => ({ type: "mousePressed", x: 10, y: 10, button: "left", clickCount: 1 }), scope: "page" },
  { method: "Input.dispatchKeyEvent", params: () => ({ type: "keyDown", key: "a", text: "a" }), scope: "page" },
  { method: "Fetch.enable", scope: "page" },
  { method: "Log.enable", scope: "page" },
  { method: "Performance.getMetrics", scope: "page" },
  { method: "Target.closeTarget", params: (c) => ({ targetId: c.targetId }), scope: "browser" },
];

async function cdpCompat(cdp: Cdp) {
  const { targetId, sessionId } = await openPage(cdp);
  const ctx: any = { targetId };
  const results: Record<string, string> = {};
  if (!sessionId) return { passed: 0, total: CDP_COMMANDS.length, results: { attach: "failed" } };
  await loadPage(cdp, sessionId, `${origin}/cdp.html`, false);
  const doc = await cdp.send("Runtime.evaluate", { expression: "document" }, sessionId);
  ctx.docObjectId = doc.result?.result?.objectId;
  for (const c of CDP_COMMANDS) {
    if (c.method === "Runtime.callFunctionOn") {
      const fresh = await cdp.send("Runtime.evaluate", { expression: "document" }, sessionId);
      ctx.docObjectId = fresh.result?.result?.objectId;
    }
    const params = c.params ? c.params(ctx) : {};
    const r = await cdp.send(c.method, params, c.scope === "page" ? sessionId : undefined, 15000);
    if (c.method === "DOM.getDocument") ctx.rootNodeId = r.result?.root?.nodeId;
    if (c.method === "DOM.querySelector") ctx.h1NodeId = r.result?.nodeId;
    results[c.method] = r.error ? `error ${r.error.code}: ${String(r.error.message).slice(0, 60)}` : "ok";
  }
  const passed = Object.values(results).filter((v) => v === "ok").length;
  return { passed, total: CDP_COMMANDS.length, results };
}

async function measureEngine(e: Engine) {
  const report: any = { engine: e.name, binaryBytes: statSync(e.path).size };
  const starts: number[] = [];
  for (let i = 0; i < runs; i++) {
    const s = await startEngine(e);
    starts.push(s.startMs);
    await stopEngine(s);
  }
  report.startupMs = { median: median(starts), runs: starts.map((x) => Math.round(x)) };

  const s = await startEngine(e);
  report.version = s.version?.Browser ?? s.version?.["User-Agent"] ?? null;
  const cdp = await Cdp.connect(s.version.webSocketDebuggerUrl);
  const pages: Record<string, any> = {};
  const list = ["blank.html", "cdp.html", "wasm-bench.html", "webgpu-readback.html", ...sitePages];
  for (const name of list) {
    const loads: number[] = [];
    let result: any = null;
    let rss = 0;
    const n = name.endsWith("-bench.html") || name.startsWith("webgpu") ? 1 : runs;
    for (let i = 0; i < n; i++) {
      const { sessionId, targetId } = await openPage(cdp);
      if (!sessionId) break;
      const r = await loadPage(cdp, sessionId, `${origin}/${name}`, /bench|webgpu/.test(name));
      loads.push(r.loadMs);
      result = r.result ?? result;
      rss = Math.max(rss, treeRssKb(s.proc.pid));
      await cdp.send("Target.closeTarget", { targetId });
    }
    pages[name] = { loadMs: { median: median(loads), runs: loads.map((x) => Math.round(x)) }, rssKbAfter: rss, result };
  }
  report.pages = pages;

  // screenshot bytes of the first site page, when the engine can
  if (sitePages.length) {
    const { sessionId, targetId } = await openPage(cdp);
    if (sessionId) {
      await loadPage(cdp, sessionId, `${origin}/${sitePages[0]}`, false);
      await sleep(500);
      const shot = await cdp.send("Page.captureScreenshot", { format: "png" }, sessionId, 60000);
      const t0 = performance.now();
      const shot2 = await cdp.send("Page.captureScreenshot", { format: "png" }, sessionId, 60000);
      const dataB64 = shot2.result?.data ?? shot.result?.data;
      if (dataB64) {
        const bytes = Buffer.from(dataB64, "base64");
        writeFileSync(join(opt("shots", tmpdir())!, `${e.name}-${sitePages[0].replace(/\W/g, "_")}.png`), bytes);
        report.screenshot = { page: sitePages[0], pngBytes: bytes.length, captureMs: Math.round(performance.now() - t0) };
      } else report.screenshot = { page: sitePages[0], error: (shot2.error ?? shot.error)?.message };
      await cdp.send("Target.closeTarget", { targetId });
    }
  }
  report.cdp = await cdpCompat(cdp);
  report.peakRssKb = treeRssKb(s.proc.pid);
  cdp.close();
  await stopEngine(s);
  return report;
}

const reports = [];
for (const e of engines) {
  try {
    reports.push(await measureEngine(e));
  } catch (err) {
    reports.push({ engine: e.name, error: String(err) });
  }
}
server.stop(true);
const json = JSON.stringify({ date: new Date().toISOString(), runs, reports }, null, 2);
if (outPath) writeFileSync(outPath, json);
console.log(json);
