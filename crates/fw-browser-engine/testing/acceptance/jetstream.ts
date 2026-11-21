// JetStream (WebKit/JetStream, 3.x) inside an engine over raw CDP, driven the way the suite's own
// tests/run-browser.mjs does: load index.html?<params>, wait for JetStream.isReady, call JetStream.start(),
// poll JetStream.isDone / JetStream.errors / JetStream.resultsObject("simple").
import { join, normalize, resolve, sep } from "node:path";
import { Cdp } from "./cdp";
import { spawn } from "./sys.ts";

export type JetStreamResult = {
  score: number | null; // the overall JetStream score as the suite itself reports it
  tests: Record<string, number>; // per-workload score
  ms: number; // wall time of the run
  revision: string | null; // git sha of the checkout
  error?: string; // set instead of throwing when the engine cannot run the suite
};

export type JetStreamOptions = { tests?: string[]; timeout_ms?: number; onProgress?: (line: string) => void };

const POLL_MS = 500;
const DEFAULT_TIMEOUT_MS = 60 * 60_000;

// What the page reports through JetStream.resultsObject("simple"): { name: { Score, ...subScores } | "FAILED" }.
export type SimpleResults = Record<string, unknown>;

const geomean = (values: number[]) =>
  values.length ? Math.exp(values.reduce((sum, value) => sum + Math.log(value), 0) / values.length) : null;

// Per-workload scores, the failed workloads, and the overall score: the number the suite prints in
// #result-summary .score (summary), or the geometric mean of the workload scores when the page shows none.
export function parseResults(simple: SimpleResults, summary?: string | null) {
  const tests: Record<string, number> = {};
  const failed: string[] = [];
  for (const [name, value] of Object.entries(simple ?? {})) {
    const score = (value as { Score?: unknown } | null)?.Score;
    if (typeof score === "number" && Number.isFinite(score) && score > 0) tests[name] = score;
    else failed.push(name);
  }
  const shown = summary ? Number(summary.trim()) : NaN;
  const score = Number.isFinite(shown) && shown > 0 ? shown : failed.length ? null : geomean(Object.values(tests));
  return { score, tests, failed };
}

const MIME: Record<string, string> = {
  html: "text/html; charset=utf-8",
  htm: "text/html; charset=utf-8",
  js: "text/javascript; charset=utf-8",
  mjs: "text/javascript; charset=utf-8",
  cjs: "text/javascript; charset=utf-8",
  css: "text/css; charset=utf-8",
  json: "application/json; charset=utf-8",
  wasm: "application/wasm",
  svg: "image/svg+xml",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  ico: "image/x-icon",
  txt: "text/plain; charset=utf-8",
  map: "application/json; charset=utf-8",
  woff: "font/woff",
  woff2: "font/woff2",
  ttf: "font/ttf",
  z: "application/octet-stream",
  data: "application/octet-stream",
  bin: "application/octet-stream",
};

// The suite's own tests/server.mjs sends these two headers (local-web-server corsOpenerPolicy/corsEmbedderPolicy).
export const ISOLATION_HEADERS = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
};

export function mimeOf(path: string): string {
  const dot = path.lastIndexOf(".");
  return (dot >= 0 && MIME[path.slice(dot + 1).toLowerCase()]) || "application/octet-stream";
}

// Static server on 127.0.0.1, ephemeral port, rooted at dir; never serves outside dir.
export function serveStatic(dir: string) {
  const root = resolve(dir);
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const headers = { ...ISOLATION_HEADERS, "Cache-Control": "no-store" };
      let rel: string;
      try {
        rel = normalize(decodeURIComponent(new URL(request.url).pathname));
      } catch {
        return new Response("bad request", { status: 400, headers });
      }
      if (request.method !== "GET" && request.method !== "HEAD") return new Response("", { status: 405, headers });
      let path = join(root, rel);
      if (path !== root && !path.startsWith(root + sep)) return new Response("forbidden", { status: 403, headers });
      if (path.endsWith(sep) || (await isDirectory(path))) path = join(path, "index.html");
      const file = Bun.file(path);
      if (!(await file.exists())) return new Response("not found", { status: 404, headers });
      const type = { "Content-Type": mimeOf(path), "Content-Length": String(file.size) };
      return new Response(request.method === "HEAD" ? null : file, { headers: { ...headers, ...type } });
    },
  });
  return { url: `http://127.0.0.1:${server.port}`, stop: () => server.stop(true) };
}

async function isDirectory(path: string) {
  try {
    return (await Bun.file(path).stat()).isDirectory();
  } catch {
    return false;
  }
}

async function gitRevision(dir: string): Promise<string | null> {
  try {
    const run = spawn(["git", "-C", dir, "rev-parse", "HEAD"], { stdout: "pipe", stderr: "ignore" });
    const out = (await new Response(run.stdout).text()).trim();
    return (await run.exited) === 0 && /^[0-9a-f]{40}/.test(out) ? out : null;
  } catch {
    return null;
  }
}

// Page side: one JSON document per poll.
const POLL = `(() => {
  const j = globalThis.JetStream;
  const out = { good: globalThis.allIsGood !== false, ready: false, done: false, errors: [], total: 0,
    results: {}, summary: null };
  if (!j) return JSON.stringify(out);
  out.ready = !!j.isReady;
  out.done = !!j.isDone;
  out.total = j.benchmarks.length;
  out.errors = j.errors.map((e) => ({ benchmark: e.benchmark, error: String(e.error), stack: String(e.stack || "") }));
  out.results = j.resultsObject("simple");
  out.summary = document.querySelector("#result-summary .score")?.textContent ?? null;
  return JSON.stringify(out);
})()`;

type Poll = {
  good: boolean;
  ready: boolean;
  done: boolean;
  errors: { benchmark: string; error: string; stack: string }[];
  total: number;
  results: SimpleResults;
  summary: string | null;
};

async function endpoint(ws: string): Promise<string> {
  if (!/^https?:/.test(ws)) return ws;
  const info = (await (await fetch(new URL("/json/version", ws))).json()) as { webSocketDebuggerUrl?: string };
  if (!info.webSocketDebuggerUrl) throw new Error(`${ws}/json/version has no webSocketDebuggerUrl`);
  return info.webSocketDebuggerUrl;
}

async function connect(url: string): Promise<Cdp> {
  let timer: Timer | undefined;
  const timeout = new Promise<never>((_, fail) => {
    timer = setTimeout(() => fail(new Error(`no CDP connection to ${url} in 15 s`)), 15_000);
  });
  try {
    return await Promise.race([Cdp.open(url), timeout]);
  } finally {
    clearTimeout(timer);
  }
}

export async function jetstream(ws: string, dir: string, opt: JetStreamOptions = {}): Promise<JetStreamResult> {
  const started = performance.now();
  const timeout = opt.timeout_ms ?? DEFAULT_TIMEOUT_MS;
  const deadline = started + timeout;
  const progress = opt.onProgress ?? (() => {});
  const result: JetStreamResult = { score: null, tests: {}, ms: 0, revision: await gitRevision(dir) };
  const fail = (message: string) => ((result.error = message), result);
  let server: ReturnType<typeof serveStatic> | undefined;
  let cdp: Cdp | undefined;
  let targetId: string | undefined;
  try {
    if (!(await Bun.file(join(dir, "index.html")).exists())) return fail(`no index.html in ${dir}`);
    server = serveStatic(dir);
    cdp = await connect(await endpoint(ws));
    const client = cdp;
    let crashed = false;
    const pageErrors: string[] = [];
    client.on((m) => {
      if (m.method === "Inspector.targetCrashed" || m.method === "Target.targetCrashed") crashed = true;
      else if (m.method === "Runtime.exceptionThrown") {
        const d = m.params?.exceptionDetails;
        pageErrors.push(String(d?.exception?.description ?? d?.text ?? "exception"));
      }
    });
    targetId = (await client.send("Target.createTarget", { url: "about:blank" })).targetId;
    const { sessionId } = await client.send("Target.attachToTarget", { targetId, flatten: true });
    await client.send("Runtime.enable", {}, sessionId);
    await client.send("Page.enable", {}, sessionId);

    const params = new URLSearchParams();
    if (opt.tests?.length) params.set("test", opt.tests.join(","));
    const url = `${server.url}/index.html${params.size ? `?${params}` : ""}`;
    progress(`navigate ${url}`);
    const nav = await client.send("Page.navigate", { url }, sessionId);
    if (nav?.errorText) return fail(`navigation to ${url} failed: ${nav.errorText}`);

    const poll = async (): Promise<Poll | null> => {
      const left = deadline - performance.now();
      if (left <= 0) return null;
      try {
        const value = await client.send(
          "Runtime.evaluate",
          { expression: POLL, returnByValue: true },
          sessionId,
          Math.min(left, 120_000),
        );
        if (value?.exceptionDetails) return null;
        return JSON.parse(value?.result?.value ?? "null");
      } catch (e) {
        if (String((e as Error).message).includes("closed")) throw e;
        return null; // page navigating, busy, or the engine does not answer yet: retry until the deadline
      }
    };
    const timedOut = (state: Poll | null, what: string) => {
      const done = state ? Object.keys(state.results).length : 0;
      const seconds = Math.round((performance.now() - started) / 1000);
      return fail(`timeout after ${seconds} s ${what}${state ? ` (${done}/${state.total} workloads)` : ""}`);
    };
    const settle = (state: Poll | null) => {
      if (!state) return;
      const partial = parseResults(state.results, null);
      result.tests = partial.tests;
      result.score = partial.score;
    };

    // Ready: the driver has prefetched every resource and built its UI.
    let state: Poll | null = null;
    for (;;) {
      if (crashed) return fail("the page crashed while loading the suite");
      state = await poll();
      if (state?.ready) break;
      if (state && !state.good) {
        const why = [...pageErrors, ...state.errors.map((e) => e.error)].join("; ");
        return fail(`the suite refused to load: ${why || "allIsGood is false"}`);
      }
      if (performance.now() >= deadline) return timedOut(state, "waiting for JetStream.isReady");
      await Bun.sleep(POLL_MS);
    }
    progress(`ready, ${state.total} workloads`);

    await client.send(
      "Runtime.evaluate",
      { expression: "JetStream.start().catch(() => {}); true", returnByValue: true },
      sessionId,
      30_000,
    );

    const seen = new Set<string>();
    for (;;) {
      await Bun.sleep(POLL_MS);
      if (crashed) {
        settle(state);
        return fail("the page crashed during the run");
      }
      state = await poll();
      if (state) {
        for (const [name, value] of Object.entries(state.results)) {
          if (seen.has(name)) continue;
          seen.add(name);
          const score = (value as { Score?: number } | null)?.Score;
          progress(`${seen.size}/${state.total} ${name} ${typeof score === "number" ? score.toFixed(3) : "FAILED"}`);
        }
        if (state.errors.length) {
          settle(state);
          const e = state.errors[0];
          return fail(`${e.benchmark}: ${e.error}`);
        }
        if (state.done) break;
      }
      if (performance.now() >= deadline) {
        settle(state);
        return timedOut(state, "running the suite");
      }
    }

    const parsed = parseResults(state!.results, state!.summary);
    result.score = parsed.score;
    result.tests = parsed.tests;
    if (parsed.failed.length) result.error = `failed workloads: ${parsed.failed.join(", ")}`;
    else if (parsed.score === null) result.error = "the suite finished without a score";
    return result;
  } catch (e) {
    return fail(String((e as Error)?.message ?? e));
  } finally {
    result.ms = Math.round(performance.now() - started);
    if (cdp && targetId) await cdp.send("Target.closeTarget", { targetId }, undefined, 5_000).catch(() => {});
    cdp?.close();
    await server?.stop();
  }
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const flag = args.find((arg) => arg.startsWith("--timeout="));
  const [endpointUrl, dir, ...tests] = args.filter((arg) => arg !== flag);
  if (!endpointUrl || !dir) {
    console.error("usage: jetstream.ts <ws-or-http-endpoint> <checkout-dir> [test...] [--timeout=<ms>]");
    process.exit(2);
  }
  const timeout_ms = flag ? Number(flag.slice("--timeout=".length)) : undefined;
  const out = await jetstream(endpointUrl, dir, { tests, timeout_ms, onProgress: (line) => console.error(line) });
  console.log(JSON.stringify(out, null, 2));
  process.exit(out.error ? 1 : 0);
}
