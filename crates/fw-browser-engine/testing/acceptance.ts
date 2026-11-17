#!/usr/bin/env bun
// The one web acceptance tool of the browser. Every engine found on this machine loads the live corpus
// (seeds + recursive link index, testing/acceptance.toml) over raw CDP and is judged on what it rendered.
// State is durable (SQLite) and shared (Redis): an interrupted index or run resumes where it stopped.
//
//   bun testing/acceptance.ts engines                engines, fronts and tools found here, with versions
//   bun testing/acceptance.ts install                fetch the newest upstream baseline of every engine (canary, latest release)
//   bun testing/acceptance.ts sites                  the corpus after the link index (resumed or reused when fresh enough)
//   bun testing/acceptance.ts run                    engines x corpus -> <out>/{report.md,report.json,<engine>/*.png}; resumes
//   bun testing/acceptance.ts bench [url...]         hyperfine on the engines' one-shot commands + process-tree RSS (default: seeds)
//   bun testing/acceptance.ts bench --jetstream      the pinned JetStream suite inside every full engine
//   bun testing/acceptance.ts lessons              where each engine fell short of the full one, learned from the runs (the router's data)
//   bun testing/acceptance.ts track                  heads, releases and channels of every upstream against our pins
//   bun testing/acceptance.ts serve [--schedule]     the live dashboard (HTML, JSON, SSE) and, with --schedule, the cron of the above
//   bun testing/acceptance.ts deploy [--apply]      Worker + tunnel config for the published host (plan only without --apply)
//   bun testing/acceptance.ts report [run]           print report.md of a run (default: the last one)
//   bun testing/acceptance.ts cookies <cdp> <file>   save the corpus cookies of a running, signed-in browser
//
// flags: --engine <name[=path]>   repeatable; default is the first binary found per engine (--all: every binary)
//        --site <substring>       repeatable filter on the corpus     --no-discover   seeds only
//        --fresh                  new index / new run instead of resuming            --install   `install` before `run`
//        --jobs <n>               pages in parallel per engine        --out <dir>     default out/acceptance/<stamp>
//        --cookies <file>         cookies set before the first page   --profile <dir> persistent profile directory
//        --db <file>  --redis <url|off>  --warmup <n>  --runs <n>  --port <n>  --test <name> (JetStream workload, repeatable)
// Exit status: 0 clean, 1 a full-engine page or a front failed, or a page that passed in the previous run fails, 2 nothing to run.
import { basename, delimiter, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { Cdp } from "./acceptance/cdp.ts";
import {
  spawn,
  cores,
  exists,
  list,
  mkdirp,
  mkdtemp,
  move,
  rmrf,
  sampleRss,
  tmpdir,
  hostname,
  loadavg,
} from "./acceptance/sys.ts";
import { Store } from "./acceptance/store.ts";

type Baseline =
  | { kind: "chrome-for-testing"; channel: string; product: string }
  | { kind: "github-release"; repo: string; asset: string };
type EngineDef = { role: "full" | "lite"; bins: string[]; args: string[]; once?: string[]; baseline?: Baseline };
export type Manifest = {
  store: { db: string; keep_runs: number };
  corpus: {
    seeds: string[];
    discover: {
      roots: string[];
      depth: number;
      per_host: number;
      drop_params: string[];
      fetch_per_host: number;
      fetch_max: number;
      ttl_s: number;
      agent: string;
    };
  };
  page: { width: number; height: number; timeout_ms: number; settle_ms: number; quiet_ms: number; retries: number };
  walls: Record<"auth" | "consent" | "refused" | "challenge", string[]>;
  session: { signed_in: string; signed_out: string };
  parity: { elements: number; text: number };
  engine: Record<string, EngineDef>;
  front: Record<string, { bins: string[] }>;
  bench: { warmup: number; runs: number; rss_runs: number };
  serve: { port: number; host: string; origin: string };
  schedule: Record<string, string>;
};
export type Site = { url: string; host: string; from: string; depth: number };
export type Engine = EngineDef & { id: string; name: string; path: string; origin: "build" | "baseline" };
export type Verdict = "pass" | "auth" | "consent" | "refused" | "fail";
export type PageResult = {
  url: string;
  verdict: Verdict;
  reason?: string;
  error?: string;
  final: string | null;
  status: number | null;
  protocol: string | null;
  attempts?: number; // 1 unless a transient failure was tried again
  redirects: number;
  loaded: boolean;
  ms: number;
  title: string;
  lang: string | null;
  session: "signed-in" | "signed-out" | null;
  elements: number;
  custom: number;
  shadow: number;
  text: number;
  links: number;
  images: number;
  scripts: number;
  frames: number;
  canvases: number;
  ttfb: number | null;
  dcl: number | null;
  load: number | null;
  fcp: number | null;
  lcp: number | null;
  requests: number;
  failed: number;
  http_errors: number;
  bytes: number;
  exceptions: number;
  console_errors: number;
  first_error?: string;
  shot: string | null;
  spread: number | null;
};
export type Running = {
  engine: Engine;
  pid: number;
  port: number;
  ws: string;
  version: string;
  start_ms: number;
  features?: Promise<Record<string, unknown>>;
  exited: () => boolean;
  peak: () => number | null;
  stop: () => Promise<void>;
};
export type FrontResult = { front: string; url: string; ok: boolean; skip?: boolean; ms: number; detail: string };
export type EngineInfo = {
  id: string;
  role: Engine["role"];
  origin: Engine["origin"];
  path: string;
  version: string;
  state: "running" | "done";
  start_ms: number;
  peak_rss_mib: number | null;
  features: Record<string, unknown>;
  fronts: FrontResult[];
};
export type Parity = { url: string; sufficient: boolean; why?: string; elements: number | null; text: number | null };
export type Report = ReturnType<typeof report>;

export const root = resolve(import.meta.dir, "..");
export const manifest = Bun.TOML.parse(
  await Bun.file(new URL("./acceptance.toml", import.meta.url)).text(),
) as Manifest;
const exe = process.platform === "win32" ? ".exe" : "";
const stamp = () =>
  new Date()
    .toISOString()
    .replace(/\.\d+Z$/, "Z")
    .replaceAll(":", "");
const median = (xs: number[]) => (xs.length ? xs.toSorted((a, b) => a - b)[xs.length >> 1] : null);
const fill = (text: string, vars: Record<string, string | number>) =>
  text.replace(/\{(\w+)\}/g, (all, k) => String(vars[k] ?? all));

async function pool<T, R>(items: T[], width: number, fn: (item: T, index: number) => Promise<R>): Promise<R[]> {
  const out = new Array<R>(items.length);
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const i = next++;
      out[i] = await fn(items[i], i);
    }
  };
  await Promise.all(Array.from({ length: Math.min(width, items.length) }, worker));
  return out;
}

export const openStore = (flag: { db?: string; redis?: string } = {}) =>
  Store.open(
    resolve(root, flag.db ?? Bun.env.ACCEPTANCE_DB ?? manifest.store.db),
    flag.redis === "off" ? false : flag.redis,
  );

// ---- corpus -------------------------------------------------------------------------------------------------

/** Absolute http(s) URL of a link, without fragment and tracking parameters; null for anything that is not a page. */
export function normalize(href: string, base: string, drop: string[]): URL | null {
  let url: URL;
  try {
    url = new URL(href.replaceAll("&amp;", "&"), base); // HTMLRewriter hands attributes over undecoded
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  const ext = url.pathname.match(/\.([a-z0-9]{1,5})$/i)?.[1].toLowerCase();
  if (ext && !["html", "htm", "aspx", "php"].includes(ext)) return null;
  url.hash = "";
  const globs = drop.map((pattern) => new Bun.Glob(pattern));
  for (const key of [...url.searchParams.keys()]) if (globs.some((g) => g.match(key))) url.searchParams.delete(key);
  return url;
}

type Discover = Manifest["corpus"]["discover"];

/** Links of an index page: from the hot cache, else revalidated (304 reuses the stored list), else fetched and parsed. */
async function linksOf(store: Store, url: string, cfg: Discover) {
  const hot = await store.cached<string[]>(`links:${url}`);
  const last = store.lastFetch(url);
  if (hot) return { status: 200, links: hot, etag: last?.etag ?? null, modified: last?.modified ?? null };
  const headers: Record<string, string> = { accept: "text/html", "user-agent": cfg.agent };
  if (last?.etag) headers["if-none-match"] = last.etag;
  if (last?.modified) headers["if-modified-since"] = last.modified;
  const res = await fetch(url, { signal: AbortSignal.timeout(20000), headers });
  const validators = {
    etag: res.headers.get("etag") ?? last?.etag ?? null,
    modified: res.headers.get("last-modified") ?? last?.modified ?? null,
  };
  let links: string[] = [];
  if (res.status === 304 && last?.links) links = JSON.parse(last.links);
  else if (res.ok && res.headers.get("content-type")?.includes("html")) {
    const seen = new Set<string>();
    const rewriter = new HTMLRewriter().on("a[href]", {
      element(a) {
        const link = normalize(a.getAttribute("href") ?? "", res.url, cfg.drop_params);
        if (link) seen.add(link.href);
      },
    });
    await rewriter.transform(res).arrayBuffer();
    links = [...seen];
  }
  if (res.ok || res.status === 304) await store.cache(`links:${url}`, links, cfg.ttl_s);
  return { status: res.status, links, ...validators };
}

/** Walks the link graph from the roots to `depth`. Every step is committed: a second call continues an unfinished walk. */
export async function index(store: Store, cfg: Discover, fresh = false): Promise<string> {
  const { crawl, reused, resumed } = store.openCrawl(stamp(), cfg.roots, cfg.ttl_s, fresh);
  if (reused) return crawl;
  if (resumed) console.error(`index ${crawl}: resuming`);
  for (;;) {
    const read = store.fetchedPerHost(crawl);
    let budget = cfg.fetch_max - [...read.values()].reduce((a, b) => a + b, 0);
    const pending = store.pendingIndex(crawl, 64);
    if (!pending.length) break;
    const batch = pending.filter((row) => {
      const n = read.get(row.host) ?? 0;
      if (budget > 0 && n < cfg.fetch_per_host) return (budget--, read.set(row.host, n + 1), true);
      store.saveIndex(crawl, row.url, { state: "leaf", fetched: null }); // over a ceiling: kept as a page, not read for links
      return false;
    });
    await pool(batch, 8, async (row) => {
      const page = await linksOf(store, row.url, cfg).catch(() => null);
      if (!page) return store.saveIndex(crawl, row.url, { state: "error" });
      const state = row.depth + 1 < cfg.depth ? "pending" : "leaf";
      store.db.transaction(() => {
        for (const url of page.links)
          store.addIndex({ crawl, url, host: new URL(url).host, parent: row.url, depth: row.depth + 1, state });
        store.saveIndex(crawl, row.url, {
          state: "done",
          status: page.status,
          etag: page.etag,
          modified: page.modified,
          links: JSON.stringify(page.links),
        });
      })();
    });
    const rows = store.index(crawl);
    store.emit("index", {
      crawl,
      known: rows.length,
      read: rows.filter((r) => r.state === "done").length,
      pending: rows.filter((r) => r.state === "pending").length,
    });
  }
  return crawl;
}

/** Seeds, then for every other host the index reached its shallowest pages. */
export async function corpus(
  store: Store,
  cfg: Manifest["corpus"],
  opt: { discover?: boolean; fresh?: boolean } = {},
): Promise<Site[]> {
  const sites: Site[] = cfg.seeds.map((url) => ({ url, host: new URL(url).host, from: "seed", depth: 0 }));
  if (opt.discover === false) return sites;
  const crawl = await index(store, cfg.discover, opt.fresh);
  const found = store.index(crawl).filter((row) => row.depth > 0);
  const ranked = found.sort(
    (a, b) =>
      a.host.localeCompare(b.host) ||
      a.depth - b.depth ||
      new URL(a.url).pathname.length - new URL(b.url).pathname.length ||
      a.url.length - b.url.length ||
      a.url.localeCompare(b.url),
  );
  const per = new Map<string, number>();
  for (const site of sites) per.set(site.host, (per.get(site.host) ?? 0) + 1);
  for (const row of ranked) {
    const n = per.get(row.host) ?? 0;
    if (n >= cfg.discover.per_host || sites.some((s) => s.url === row.url)) continue;
    per.set(row.host, n + 1);
    sites.push({ url: row.url, host: row.host, from: row.parent ?? "root", depth: row.depth });
  }
  return sites;
}

// ---- engines ------------------------------------------------------------------------------------------------

/** Where our own builds land: the GN out directory (defaults read from build/engines/build.sh) and cargo's. */
let buildDirsMemo: Promise<string[]> | undefined;
const buildDirs = () =>
  (buildDirsMemo ??= (async () => {
    const script = await Bun.file(join(root, "build/engines/build.sh")).text();
    const value = (name: string) => Bun.env[name] ?? script.match(new RegExp(`\\$\\{${name}:-([^}]+)\\}`))?.[1];
    const [src, out] = [value("CHROMIUM_SRC"), value("OUT")];
    const dirs = [join(Bun.env.CARGO_TARGET_DIR ?? join(root, "target"), "release")];
    if (src && out) dirs.unshift(join(src, out));
    return dirs;
  })());

/** Where baselines live: $BROWSER_ENGINES (a path list), else out/engines, which `install` fills. */
const baselineDirs = () => (Bun.env.BROWSER_ENGINES ?? join(root, "out/engines")).split(delimiter).filter(Boolean);

async function* binaries(bins: string[]): AsyncGenerator<Pick<Engine, "path" | "origin">> {
  for (const dir of await buildDirs())
    for (const bin of bins)
      if (await exists(join(dir, bin + exe))) yield { path: join(dir, bin + exe), origin: "build" };
  const newestFirst = (a: string, b: string) => b.localeCompare(a, undefined, { numeric: true });
  const scan = async (cwd: string, pattern: string) =>
    (await exists(cwd))
      ? (await Array.fromAsync(new Bun.Glob(pattern).scan({ cwd, absolute: true }))).sort(newestFirst)
      : [];
  for (const dir of baselineDirs())
    for (const bin of bins)
      for (const depth of ["", "*/", "*/*/"])
        for (const path of await scan(dir, depth + bin + exe)) yield { path, origin: "baseline" };
  for (const bin of bins) {
    const path = Bun.which(bin);
    if (path) yield { path, origin: "baseline" };
  }
  if (process.platform === "win32")
    for (const key of ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"])
      for (const bin of bins)
        for (const path of await scan(Bun.env[key] ?? "", `*/*/Application/${bin}.exe`))
          yield { path, origin: "baseline" };
}

/** A helper program (hyperfine, agent-browser): our build of it first, then $PATH. */
const tool = async (bin: string) => {
  for (const dir of await buildDirs()) if (await exists(join(dir, bin + exe))) return join(dir, bin + exe);
  return Bun.which(bin);
};

/** `--engine name` keeps an engine, `--engine name=path` pins its binary; without --all the first binary wins. */
export async function engines(select: string[] = [], all = false): Promise<Engine[]> {
  const pinned = new Map(select.map((s) => s.split(/=(.*)/s) as [string, string | undefined]));
  const builds = (await buildDirs()).map((dir) => resolve(dir));
  const out: Engine[] = [];
  for (const [name, def] of Object.entries(manifest.engine)) {
    if (pinned.size && !pinned.has(name)) continue;
    const pin = pinned.get(name);
    const found = pin
      ? [{ path: resolve(pin), origin: builds.some((d) => resolve(pin).startsWith(d)) ? "build" : "baseline" } as const]
      : [...new Map((await Array.fromAsync(binaries(def.bins))).map((b) => [b.path, b])).values()];
    for (const [i, bin] of (all ? found : found.slice(0, 1)).entries())
      out.push({ ...def, ...bin, name, id: i ? `${name}-${i + 1}` : name });
  }
  return out;
}

/** Fetches the newest upstream build of every engine that declares a baseline; older copies beyond two are removed. */
async function install(select: string[] = []) {
  const dir = baselineDirs()[0];
  const token = Bun.env.GH_TOKEN ?? Bun.env.GITHUB_TOKEN;
  const os = { linux: "linux", darwin: "macos", win32: "windows" }[process.platform as string] ?? process.platform;
  const arch = { x64: "x86_64", arm64: "aarch64" }[process.arch as string] ?? process.arch;
  for (const [name, def] of Object.entries(manifest.engine)) {
    const base = def.baseline;
    if (!base || (select.length && !select.some((s) => s.split("=")[0] === name))) continue;
    try {
      let [product, version, url] = ["", "", ""];
      if (base.kind === "chrome-for-testing") {
        const feed = (await (
          await fetch(
            "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json",
          )
        ).json()) as any;
        const platform = {
          linux: "linux64",
          darwin: process.arch === "arm64" ? "mac-arm64" : "mac-x64",
          win32: "win64",
        }[process.platform as string];
        const channel = feed.channels[base.channel];
        [product, version] = [base.product, channel.version];
        url = channel.downloads[base.product].find((d: { platform: string }) => d.platform === platform).url;
      } else {
        const api = `https://api.github.com/repos/${base.repo}/releases/latest`;
        const latest = (await (
          await fetch(api, { headers: token ? { authorization: `Bearer ${token}` } : {} })
        ).json()) as any;
        const asset = latest.assets?.find((a: { name: string }) => a.name === fill(base.asset, { arch, os }));
        if (!asset)
          throw new Error(`release ${latest.tag_name ?? latest.message} has no ${fill(base.asset, { arch, os })}`);
        [product, version, url] = [basename(base.repo), latest.tag_name, asset.browser_download_url];
      }
      const target = join(dir, `${product}-${version}`);
      if (await exists(target)) {
        console.log(`${name}: ${product} ${version} already in ${target}`);
        continue;
      }
      const temp = `${target}.partial`;
      await rmrf(temp);
      await mkdirp(temp);
      const bytes = await (await fetch(url)).bytes();
      if (url.endsWith(".zip")) {
        const zip = join(temp, "archive.zip");
        await Bun.write(zip, bytes);
        const unzip = Bun.which("unzip") ? ["unzip", "-oq", zip, "-d", temp] : ["tar", "-xf", zip, "-C", temp];
        if ((await spawn(unzip, { stdout: "ignore", stderr: "inherit" }).exited) !== 0)
          throw new Error(`cannot extract ${url}`);
        await rmrf(zip);
      } else await new Bun.Archive(bytes).extract(temp);
      await move(temp, target);
      console.log(`${name}: ${product} ${version} -> ${target}`);
      const copies = (await list(dir)).filter((d) => d.startsWith(`${product}-`) && !d.endsWith(".partial"));
      for (const old of copies.sort((a, b) => b.localeCompare(a, undefined, { numeric: true })).slice(2))
        await rmrf(join(dir, old));
    } catch (error) {
      console.error(`${name}: baseline not installed: ${(error as Error).message}`);
    }
  }
}

function freePort(): number {
  const server = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
  const { port } = server;
  server.stop(true);
  return port;
}

export async function launch(
  engine: Engine,
  opt: { profile?: string; log?: string; extra?: string[] } = {},
): Promise<Running> {
  const port = freePort();
  const temp = opt.profile ? null : await mkdtemp("acceptance-");
  const vars = { port, profile: opt.profile ?? temp!, ...manifest.page };
  const argv = [engine.path, ...engine.args.map((a) => fill(a, vars)), ...(opt.extra ?? [])];
  const t0 = performance.now();
  const proc = spawn(argv, { stdin: "ignore", stdout: "ignore", stderr: opt.log ? Bun.file(opt.log) : "ignore" });
  const rss = sampleRss(proc.pid, 100);
  // A launcher that hands over to another process and leaves with 0 (chrome.exe on Windows) is not a death.
  const died = () => proc.signalCode !== null || (proc.exitCode ?? 0) !== 0;
  const cleanup = async () => {
    rss.stop();
    proc.kill();
    await proc.exited;
    if (temp) await rmrf(temp);
  };
  try {
    for (;;) {
      if (died()) throw new Error(`${engine.id} exited with ${proc.exitCode ?? proc.signalCode} before opening CDP`);
      if (performance.now() - t0 > 30000)
        throw new Error(`${engine.id} opened no CDP endpoint on port ${port} in 30 s`);
      try {
        const res = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1000) });
        const info = (await res.json()) as { Browser: string; webSocketDebuggerUrl: string };
        return {
          engine,
          port,
          pid: proc.pid,
          ws: info.webSocketDebuggerUrl,
          version: info.Browser,
          start_ms: Math.round(performance.now() - t0),
          exited: died,
          peak: () => (rss.peak() === null ? null : Math.round((rss.peak()! / 1024) * 10) / 10),
          stop: async () => {
            const cdp = await Cdp.open(info.webSocketDebuggerUrl).catch(() => null);
            await cdp?.send("Browser.close", {}, undefined, 3000).catch(() => {});
            cdp?.close();
            await Promise.race([proc.exited, Bun.sleep(3000)]);
            await cleanup();
          },
        };
      } catch {
        await Bun.sleep(20);
      }
    }
  } catch (error) {
    await cleanup();
    throw error;
  }
}

// ---- one page -----------------------------------------------------------------------------------------------

// Runs inside the page, in any engine: every read is guarded, a missing API yields null.
const facts = (session: Manifest["session"]) => `(async () => {
  const t = (f, d = null) => { try { const v = f(); return v === undefined ? d : v; } catch { return d; } };
  const ms = (v) => (typeof v === "number" && v > 0 ? Math.round(v) : null);
  const nav = t(() => performance.getEntriesByType("navigation")[0], {}) || {};
  const paint = t(() => Object.fromEntries(performance.getEntriesByType("paint").map((e) => [e.name, e.startTime])), {});
  const lcp = await new Promise((done) => {
    let last = null;
    const po = new PerformanceObserver((list) => { for (const e of list.getEntries()) last = e.startTime; });
    po.observe({ type: "largest-contentful-paint", buffered: true });
    setTimeout(() => { po.disconnect(); done(last); }, 50);
  }).catch(() => null);
  const all = t(() => [...document.getElementsByTagName("*")], []);
  const count = (selector) => t(() => document.querySelectorAll(selector).length, 0);
  return {
    final: location.href, title: t(() => document.title, ""), lang: t(() => document.documentElement.lang || null),
    session: count(${JSON.stringify(session.signed_in)}) ? "signed-in" : count(${JSON.stringify(session.signed_out)}) ? "signed-out" : null,
    elements: all.length, custom: all.filter((e) => e.localName.includes("-")).length, shadow: all.filter((e) => e.shadowRoot).length,
    text: t(() => (document.body.innerText ?? document.body.textContent ?? "").length, 0),
    links: count("a[href]"), images: count("img"), scripts: count("script"), frames: count("iframe"), canvases: count("canvas"),
    status: t(() => nav.responseStatus || null), ttfb: ms(nav.responseStart), dcl: ms(nav.domContentLoadedEventEnd), load: ms(nav.loadEventEnd),
    fcp: ms(paint["first-contentful-paint"]), lcp: ms(lcp),
  };
})()`;

// What the engine itself can do, asked once per engine from the first page (a secure context).
const features = `(async () => {
  const t = async (f) => { try { return await f(); } catch { return false; } };
  return {
    wasm: await t(() => WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]))),
    webgpu: await t(async () => !!(await Promise.race([navigator.gpu.requestAdapter(), new Promise((r) => setTimeout(r, 5000, null))]))),
    webgl2: await t(() => !!document.createElement("canvas").getContext("webgl2")),
    service_worker: await t(() => "serviceWorker" in navigator),
    custom_elements: await t(() => typeof customElements.define === "function"),
    view_transitions: await t(() => typeof document.startViewTransition === "function"),
    ua: await t(() => navigator.userAgent),
  };
})()`;

/** Luma spread (0..255) of a screenshot, read from a 64x64 reduction: 0 is one flat colour, nothing was painted. */
export async function spread(image: Uint8Array): Promise<number | null> {
  try {
    const png = await new Bun.Image(image).resize(64, 64).png().bytes();
    const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
    const idat: Uint8Array[] = [];
    let [width, channels] = [0, 0];
    for (let at = 8; at < png.length; at += 12 + view.getUint32(at)) {
      const type = String.fromCharCode(...png.subarray(at + 4, at + 8));
      const body = png.subarray(at + 8, at + 8 + view.getUint32(at));
      if (type === "IDAT") idat.push(body);
      if (type !== "IHDR") continue;
      width = view.getUint32(at + 8);
      channels = [1, 0, 3, 0, 2, 0, 4][body[9]]; // grey, -, rgb, palette, grey+alpha, -, rgba
      if (body[8] !== 8 || body[12] !== 0 || !channels) return null; // 8-bit, not interlaced, not indexed
    }
    const data = Bun.inflateSync(await new Blob(idat as unknown as BlobPart[]).bytes());
    const stride = width * channels;
    const above = new Uint8Array(stride);
    let [low, high] = [255, 0];
    for (let at = 0; at + stride < data.length; at += stride + 1) {
      const filter = data[at];
      const row = data.subarray(at + 1, at + 1 + stride);
      for (let i = 0; i < stride; i++) {
        const [a, b, c] = [i < channels ? 0 : row[i - channels], above[i], i < channels ? 0 : above[i - channels]];
        const [pa, pb, pc] = [Math.abs(b - c), Math.abs(a - c), Math.abs(a + b - 2 * c)];
        const paeth = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
        row[i] = (row[i] + [0, a, b, (a + b) >> 1, paeth][filter]) & 255;
      }
      above.set(row);
      for (let i = 0; i < stride; i += channels) {
        const luma = channels < 3 ? row[i] : (row[i] * 54 + row[i + 1] * 183 + row[i + 2] * 19) >> 8;
        low = Math.min(low, luma);
        high = Math.max(high, luma);
      }
    }
    return high - low;
  } catch {
    return null; // no decoder for this image here: the paint check is skipped, not failed
  }
}

/** Names what happened. `refused` is the server's answer (4xx, 5xx, a robot wall), `fail` is the engine's. */
export function judge(r: PageResult, walls: Manifest["walls"]): Pick<PageResult, "verdict" | "reason"> {
  if (r.error) return { verdict: "fail", reason: r.error };
  const at = r.final ?? r.url;
  const wall = (["refused", "auth", "consent"] as const).find((k) => walls[k].some((p) => new RegExp(p).test(at)));
  if (wall === "refused" || (r.status ?? 0) >= 400) return { verdict: "refused", reason: `http ${r.status} at ${at}` };
  if (walls.challenge.some((p) => new RegExp(p, "i").test(r.title)))
    return { verdict: "refused", reason: `challenge page "${r.title}"` };
  if (r.elements <= 3) return { verdict: "fail", reason: "empty document" }; // html, head, body
  if (r.spread === 0) return { verdict: "fail", reason: "blank paint" };
  if (wall) return { verdict: wall };
  return { verdict: "pass", reason: r.loaded ? undefined : "no load event" };
}

/** Failures that say nothing about the engine: the network moved, the peer reset, or the engine was starved of CPU. */
export const isTransient = (reason = "") =>
  /ERR_NETWORK_CHANGED|ERR_CONNECTION_(RESET|CLOSED|TIMED_OUT)|ERR_TIMED_OUT|ERR_INTERNET_DISCONNECTED|ERR_NAME_NOT_RESOLVED|no answer in/.test(
    reason,
  );

export async function probeRetrying(run: Running, url: string, shot: string | null, retries = manifest.page.retries) {
  let page = await probe(run, url, shot);
  for (
    let attempt = 1;
    attempt <= retries && page.verdict === "fail" && isTransient(page.reason) && !run.exited();
    attempt++
  ) {
    await Bun.sleep(1000 * attempt);
    page = Object.assign(await probe(run, url, shot), { attempts: attempt + 1 });
  }
  return page;
}

export async function probe(run: Running, url: string, shot: string | null, settle = true): Promise<PageResult> {
  const { timeout_ms, settle_ms, quiet_ms, width, height } = manifest.page;
  const r: PageResult = {
    url,
    verdict: "fail",
    final: null,
    status: null,
    protocol: null,
    redirects: 0,
    loaded: false,
    ms: 0,
    title: "",
    lang: null,
    session: null,
    elements: 0,
    custom: 0,
    shadow: 0,
    text: 0,
    links: 0,
    images: 0,
    scripts: 0,
    frames: 0,
    canvases: 0,
    ttfb: null,
    dcl: null,
    load: null,
    fcp: null,
    lcp: null,
    requests: 0,
    failed: 0,
    http_errors: 0,
    bytes: 0,
    exceptions: 0,
    console_errors: 0,
    shot: null,
    spread: null,
  };
  let cdp: Cdp | undefined;
  let targetId: string | undefined;
  try {
    cdp = await Cdp.open(run.ws);
    targetId = (await cdp.send("Target.createTarget", { url: "about:blank" })).targetId;
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const send = (method: string, params?: object, timeout?: number) => cdp!.send(method, params, sessionId, timeout);
    const documents: { id: string; frame: string; status: number | null; protocol: string | null }[] = [];
    const inflight = new Set<string>();
    let [loaded, crashed, active] = [false, false, performance.now()];
    cdp.on(({ method, params: p, sessionId: from }) => {
      if (from !== sessionId) return;
      if (method === "Page.loadEventFired") loaded = true;
      else if (method === "Inspector.targetCrashed") crashed = true;
      else if (method === "Page.javascriptDialogOpening")
        send("Page.handleJavaScriptDialog", { accept: false }).catch(() => {});
      else if (method === "Runtime.exceptionThrown") {
        r.exceptions++;
        r.first_error ??= String(p.exceptionDetails?.exception?.description ?? p.exceptionDetails?.text).slice(0, 200);
      } else if (method === "Runtime.consoleAPICalled") r.console_errors += p.type === "error" ? 1 : 0;
      else if (method === "Log.entryAdded") r.console_errors += p.entry?.level === "error" ? 1 : 0;
      else if (method === "Network.requestWillBeSent") {
        active = performance.now();
        inflight.add(p.requestId);
        if (p.redirectResponse) r.redirects += documents.some((d) => d.id === p.requestId) ? 1 : 0;
        else r.requests++;
        if (p.type === "Document" && !documents.some((d) => d.id === p.requestId))
          documents.push({ id: p.requestId, frame: p.frameId, status: null, protocol: null });
      } else if (method === "Network.responseReceived") {
        const doc = documents.find((d) => d.id === p.requestId);
        if (doc) [doc.status, doc.protocol] = [p.response.status, p.response.protocol ?? null];
        if (p.response.status >= 400) r.http_errors++;
      } else if (method === "Network.loadingFinished" || method === "Network.loadingFailed") {
        active = performance.now();
        inflight.delete(p.requestId);
        if (method === "Network.loadingFinished") r.bytes += p.encodedDataLength ?? 0;
        else if (!p.canceled) r.failed++;
      }
    });
    for (const domain of ["Page", "Runtime", "Network", "Log"]) await send(`${domain}.enable`).catch(() => {});
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false }).catch(
      () => {},
    );

    const t0 = performance.now();
    const nav = await send("Page.navigate", { url }, timeout_ms);
    if (nav.errorText) throw new Error(nav.errorText);
    while (!loaded && !crashed && !run.exited() && performance.now() - t0 < timeout_ms) await Bun.sleep(20);
    r.loaded = loaded;
    r.ms = Math.round(performance.now() - t0);
    if (crashed || run.exited()) throw new Error(crashed ? "renderer crashed" : "engine exited");
    const until = performance.now() + (settle ? settle_ms : 0);
    while (performance.now() < until && (inflight.size || performance.now() - active < quiet_ms)) await Bun.sleep(20);

    const evaluate = async (expression: string) => {
      const out = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      if (out.exceptionDetails)
        throw new Error(`page script: ${out.exceptionDetails.exception?.description ?? out.exceptionDetails.text}`);
      return out.result.value;
    };
    const { status, ...seen } = await evaluate(facts(manifest.session));
    Object.assign(r, seen);
    const main = documents.findLast((d) => d.frame === nav.frameId);
    r.status = main?.status ?? status;
    r.protocol = main?.protocol ?? null;
    run.features ??= evaluate(features).catch(() => ({}));
    await run.features;
    if (shot) {
      const png = await send("Page.captureScreenshot", { format: "png" }).catch(() => null); // an engine without paint has none
      if (png?.data) {
        const bytes = Buffer.from(png.data, "base64");
        await Bun.write(shot, bytes);
        r.shot = shot;
        r.spread = await spread(bytes);
      }
    }
  } catch (error) {
    r.error = (error as Error).message;
  } finally {
    if (targetId) await cdp?.send("Target.closeTarget", { targetId }, undefined, 5000).catch(() => {});
    cdp?.close();
  }
  return Object.assign(r, judge(r, manifest.walls));
}

/** Lite is enough for a page when it ends where the full engine ends, with a comparable document. */
export function parity(lite: PageResult, full: PageResult, min: Manifest["parity"]): Parity {
  const ratio = (a: number, b: number) => (b ? Math.round((a / b) * 100) / 100 : null);
  const [elements, text] = [ratio(lite.elements, full.elements), ratio(lite.text, full.text)];
  const host = (r: PageResult) => new URL(r.final ?? r.url).host;
  const why =
    lite.verdict === "fail"
      ? lite.reason
      : lite.verdict !== full.verdict
        ? `lite ${lite.verdict}, full ${full.verdict}`
        : host(lite) !== host(full)
          ? `lite at ${host(lite)}, full at ${host(full)}`
          : (elements ?? 1) < min.elements
            ? `${elements} of the elements`
            : (text ?? 1) < min.text
              ? `${text} of the text`
              : undefined;
  return { url: full.url, sufficient: !why, why, elements, text };
}

/** What a difference between lite and full means, from the reason `parity` or `judge` gave. */
export function classify(why: string): string {
  if (/no answer|timeout|exited|crashed|opened no CDP/i.test(why)) return "hang";
  if (/empty document/.test(why)) return "empty";
  if (/blank paint/.test(why)) return "blank";
  if (/^lite \w+, full \w+/.test(why)) return "verdict-differs";
  if (/^lite at /.test(why)) return "redirect-differs";
  if (/of the (elements|text)/.test(why)) return "low-parity";
  return "other";
}

/** What the run teaches the router: per host and engine, why the engine could not do what the full one did. */
export function lessonsOf(rep: NonNullable<Report>): { host: string; engine: string; kind: string; detail: string }[] {
  const out: { host: string; engine: string; kind: string; detail: string }[] = [];
  const [full, lite] = [rep.engines.find((e) => e.role === "full"), rep.engines.find((e) => e.role === "lite")];
  for (const [i, site] of rep.corpus.entries()) {
    const host = new URL(site.url).host;
    if (lite?.pages[i] && full?.pages[i]) {
      const p = parity(lite.pages[i]!, full.pages[i]!, manifest.parity);
      if (!p.sufficient) out.push({ host, engine: lite.id, kind: classify(p.why!), detail: p.why! });
    }
    if (full?.pages[i]?.verdict === "fail")
      out.push({
        host,
        engine: full.id,
        kind: classify(full.pages[i]!.reason ?? ""),
        detail: full.pages[i]!.reason ?? "",
      });
  }
  return out;
}

// ---- fronts -------------------------------------------------------------------------------------------------

async function capture(argv: string[], timeout: number): Promise<{ code: number; out: string; err: string }> {
  const proc = spawn(argv, { stdin: "ignore", stdout: "pipe", stderr: "pipe", timeout });
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { code, out, err };
}

async function front(name: string, run: Running, url: string, dir: string, index: number): Promise<FrontResult> {
  const t0 = performance.now();
  const done = (ok: boolean, detail: string, skip = false): FrontResult => ({
    front: name,
    url,
    ok,
    ...(skip && { skip }),
    ms: Math.round(performance.now() - t0),
    detail,
  });
  const budget = manifest.page.timeout_ms * 2;
  try {
    if (name === "agent") {
      const bin = (await Promise.all(manifest.front.agent.bins.map(tool))).find(Boolean);
      if (!bin) return done(false, "agent-browser not found");
      const call = async (...args: string[]) => {
        const session = `acceptance-${process.pid}-${index}`;
        const { out, err } = await capture(
          [bin, "--session", session, "--cdp", String(run.port), "--json", ...args],
          budget,
        );
        const reply = JSON.parse(out || "{}") as { success?: boolean; data?: any; error?: unknown };
        if (!reply.success) throw new Error(`${args[0]}: ${JSON.stringify(reply.error ?? err.slice(0, 200))}`);
        return reply.data;
      };
      try {
        const page = await call("open", url);
        const snapshot = await call("snapshot", "-i");
        const refs = Object.keys(snapshot.refs ?? {}).length;
        return done(refs > 0, `${refs} interactive refs, "${page.title}"`);
      } finally {
        await call("close").catch(() => {});
      }
    }
    const png = join(dir, `webview-${index}.png`);
    const { code, out, err } = await capture(
      [process.execPath, import.meta.path, "webview", run.engine.path, url, png],
      budget,
    );
    if (code !== 0) {
      const why =
        err
          .split("\n")
          .find((l) => l.startsWith("error:"))
          ?.slice(7) ??
        err.trim().split("\n")[0] ??
        `exit ${code}`;
      return done(false, why, /closed the pipe|Failed to spawn/.test(why)); // the embedder could not start the browser here: not a verdict on the page
    }
    const seen = JSON.parse(out) as { title: string; elements: number; bytes: number };
    const painted = await spread(await Bun.file(png).bytes());
    return done(seen.elements > 3 && painted !== 0, `${seen.elements} elements, ${seen.bytes} B png, "${seen.title}"`);
  } catch (error) {
    return done(false, (error as Error).message);
  }
}

/** Child process of the webview front: Bun allows one browser per process, so each page gets its own. */
async function webview([bin, url, png]: string[]) {
  const argv = manifest.engine.blink.args.filter((a) => a.startsWith("--") && !a.includes("{") && a !== "--headless");
  const { width, height } = manifest.page;
  const view = new Bun.WebView({ width, height, backend: { type: "chrome", path: bin, argv } });
  await view.navigate(url);
  const seen = (await view.evaluate(
    "({ title: document.title, elements: document.getElementsByTagName('*').length })",
  )) as object;
  const shot = await view.screenshot({ encoding: "buffer" });
  await Bun.write(png, shot);
  console.log(JSON.stringify({ ...seen, bytes: shot.length }));
  view.close();
}

// ---- run and report -----------------------------------------------------------------------------------------

const slug = (url: string) => {
  const { host, pathname } = new URL(url);
  return `${(host + pathname)
    .replace(/[^a-z0-9.-]+/gi, "_")
    .replace(/_$/, "")
    .slice(0, 80)}-${Bun.hash(url).toString(36).slice(0, 6)}`;
};

/** A run as stored, whether finished or not: engines in run order, pages in corpus order (null = not loaded yet). */
export function report(store: Store, id?: string) {
  const all = store.runs(200);
  const run = id ? all.find((r) => r.id === id) : all[0];
  if (!run) return null;
  const sites = run.meta.corpus as Site[];
  const engines = store.engines<EngineInfo>(run.id).map((info) => {
    const stored = store.pages<PageResult>(run.id, info.id);
    const pages = sites.map((site) => stored.get(site.url) ?? null);
    const counts: Record<Verdict, number> = { pass: 0, auth: 0, consent: 0, refused: 0, fail: 0 };
    for (const page of pages) if (page) counts[page.verdict]++;
    return { ...info, counts, median_ms: median(pages.flatMap((p) => (p?.loaded ? [p.ms] : []))), pages };
  });
  const [full, lite] = [engines.find((e) => e.role === "full"), engines.find((e) => e.role === "lite")];
  const pairs =
    full && lite
      ? sites.flatMap((_, i) =>
          lite.pages[i] && full.pages[i] ? [parity(lite.pages[i]!, full.pages[i]!, manifest.parity)] : [],
        )
      : [];
  const previous = all.find((r) => r.host === run.host && r.finished && r.started < run.started);
  const regressions: string[] = [];
  for (const engine of previous ? engines : []) {
    const before = store.pages<PageResult>(previous!.id, engine.id);
    for (const page of engine.pages) {
      const was = page && before.get(page.url);
      if (was && was.verdict !== "fail" && page.verdict === "fail")
        regressions.push(`\`${engine.id}\` ${page.url}: ${was.verdict} in ${previous!.id}, fail now (${page.reason})`);
    }
  }
  const failed = engines
    .filter((e) => e.role === "full")
    .flatMap((e) => [...e.pages.filter((p) => p?.verdict === "fail"), ...e.fronts.filter((f) => !f.ok && !f.skip)]);
  return {
    stamp: run.id,
    started: run.started,
    finished: run.finished,
    host: run.host,
    platform: run.meta.platform as string,
    bun: run.meta.bun as string,
    ok: !failed.length && !regressions.length,
    previous: previous?.id ?? null,
    corpus: sites,
    engines,
    parity: pairs,
    regressions,
  };
}

export function markdown(rep: NonNullable<Report>): string {
  const row = (...cells: unknown[]) => `| ${cells.map((c) => String(c ?? "").replaceAll("|", "\\|")).join(" | ")} |`;
  const seeds = rep.corpus.filter((s) => s.from === "seed").length;
  const lines = [
    `# Acceptance ${rep.stamp}`,
    "",
    `${rep.host} (${rep.platform}), bun ${rep.bun}; ${rep.corpus.length} pages on ${new Set(rep.corpus.map((s) => s.host)).size} hosts ` +
      `(${seeds} seeds, ${rep.corpus.length - seeds} indexed from ${manifest.corpus.discover.roots.join(", ")}).${rep.finished ? "" : " Unfinished."}`,
    "",
    "## Engines",
    "",
    row(
      "engine",
      "role",
      "origin",
      "version",
      "start ms",
      "peak RSS MiB",
      "pass",
      "auth",
      "consent",
      "refused",
      "fail",
      "median load ms",
    ),
    row(...Array(12).fill("---")),
    ...rep.engines.map((e) =>
      row(
        e.id,
        e.role,
        e.origin,
        e.version,
        e.start_ms,
        e.peak_rss_mib,
        e.counts.pass,
        e.counts.auth,
        e.counts.consent,
        e.counts.refused,
        e.counts.fail,
        e.median_ms,
      ),
    ),
    "",
    ...rep.engines.map(
      (e) =>
        `- \`${e.id}\` ${e.path}: ${
          Object.entries(e.features)
            .map(([k, v]) => `${k} ${v}`)
            .join(", ") || "no feature report"
        }`,
    ),
  ];
  const fronts = rep.engines.flatMap((e) =>
    e.fronts.map((f) => row(e.id, f.front, f.ok ? "ok" : f.skip ? "SKIP" : "FAIL", f.ms, f.url, f.detail)),
  );
  if (fronts.length)
    lines.push(
      "",
      "## Fronts",
      "",
      row("engine", "front", "result", "ms", "page", "detail"),
      row(...Array(6).fill("---")),
      ...fronts,
    );
  if (rep.parity.length) {
    const enough = rep.parity.filter((p) => p.sufficient).length;
    lines.push(
      "",
      "## Lite against full",
      "",
      `The lite engine is enough for ${enough} of ${rep.parity.length} pages${enough < rep.parity.length ? "; the others escalate to the full engine:" : "."}`,
      "",
    );
    lines.push(...rep.parity.filter((p) => !p.sufficient).map((p) => `- ${p.url}: ${p.why}`));
  }
  const bad = rep.engines.flatMap((e) =>
    e.pages.flatMap((p) =>
      p && (p.verdict === "fail" || p.verdict === "refused")
        ? [`- \`${e.id}\` ${p.verdict} ${p.url}: ${p.reason}`]
        : [],
    ),
  );
  if (bad.length) lines.push("", "## Failed and refused", "", ...bad);
  if (rep.regressions.length)
    lines.push("", `## Regressions against ${rep.previous}`, "", ...rep.regressions.map((r) => `- ${r}`));
  const cell = (p: PageResult | null) =>
    p
      ? `${p.verdict} ${p.status ?? "-"} ${p.ms} ms ${p.elements} el${p.session ? ` ${p.session}` : ""}${p.shot ? ` [png](${p.shot})` : ""}`
      : "";
  lines.push(
    "",
    "## Matrix",
    "",
    row("page", ...rep.engines.map((e) => e.id), "title"),
    row(...Array(rep.engines.length + 2).fill("---")),
  );
  for (const [i, site] of rep.corpus.entries())
    lines.push(
      row(
        site.url,
        ...rep.engines.map((e) => cell(e.pages[i])),
        rep.engines.map((e) => e.pages[i]?.title).find(Boolean),
      ),
    );
  return lines.join("\n") + "\n";
}

async function run(flag: Flags) {
  const store = await openStore(flag);
  if (flag.install) await install(flag.engine);
  const found = await engines(flag.engine, flag.all);
  const host = hostname();
  const open = flag.fresh ? null : store.unfinishedRun(host);
  let sites = open?.meta.corpus as Site[] | undefined;
  if (!sites) {
    const all = await corpus(store, manifest.corpus, { discover: !flag["no-discover"], fresh: flag.fresh });
    sites = flag.site?.length ? all.filter((s) => flag.site!.some((part) => s.url.includes(part))) : all;
  }
  if (!sites.length || !found.length) {
    console.error(
      `nothing to run: ${sites.length} pages, ${found.length} engines (see \`engines\`, \`install\` and \`sites\`)`,
    );
    process.exit(2);
  }
  const id = stamp();
  const meta = {
    platform: `${process.platform} ${process.arch}`,
    bun: `${Bun.version}+${Bun.revision.slice(0, 9)}`,
    out: resolve(flag.out ?? join(root, "out/acceptance", id)),
    corpus: sites,
  };
  const row = open ?? store.createRun(id, host, meta);
  const out = row.meta.out as string;
  const jobs = Number(flag.jobs ?? Math.max(2, Math.min(8, cores() >> 1)));
  const cookies = flag.cookies ? ((await Bun.file(flag.cookies).json()) as object[]) : null;
  console.error(
    `run ${row.id}${open ? " (resumed)" : ""}: ${sites.length} pages, ${found.map((e) => e.id).join(" + ")}, store ${store.redis ? `sqlite + ${store.redis}` : "sqlite"}`,
  );
  store.emit("run", {
    id: row.id,
    state: open ? "resumed" : "started",
    pages: sites.length,
    engines: found.map((e) => e.id),
  });

  let current: Running | null = null;
  process.on("SIGINT", () => void (current ? current.stop() : Promise.resolve()).finally(() => process.exit(130)));
  for (const engine of found) {
    const dir = join(out, engine.id);
    await mkdirp(dir);
    const before = store.engine<EngineInfo>(row.id, engine.id);
    const kept = store.pages<PageResult>(row.id, engine.id);
    if (before?.state === "done" && sites.every((s) => kept.has(s.url))) continue; // finished before the interruption
    const running = (current = await launch(engine, { profile: flag.profile, log: join(dir, "engine.log") }));
    try {
      if (before && before.version !== running.version) store.dropPages(row.id, engine.id); // the binary changed meanwhile
      const done = store.pages<PageResult>(row.id, engine.id);
      const info: EngineInfo = {
        id: engine.id,
        role: engine.role,
        origin: engine.origin,
        path: engine.path,
        version: running.version,
        state: "running",
        start_ms: running.start_ms,
        peak_rss_mib: null,
        features: {},
        fronts: [],
      };
      store.saveEngine(row.id, engine.id, info);
      store.emit("engine", { run: row.id, ...info });
      const todo = sites.filter((s) => !done.has(s.url));
      console.error(
        `${engine.id}: ${running.version} (${engine.path}), ${todo.length} of ${sites.length} pages to load, ${jobs} at a time`,
      );
      if (cookies) {
        const cdp = await Cdp.open(running.ws);
        await cdp.send("Storage.setCookies", { cookies });
        cdp.close();
      }
      await pool(todo, jobs, async (site) => {
        const page = await probeRetrying(running, site.url, join(dir, `${slug(site.url)}.png`));
        if (page.shot) page.shot = `${engine.id}/${basename(page.shot)}`;
        store.savePage(row.id, engine.id, page);
        store.emit("page", {
          run: row.id,
          engine: engine.id,
          url: page.url,
          verdict: page.verdict,
          status: page.status,
          ms: page.ms,
          elements: page.elements,
          title: page.title,
          reason: page.reason,
        });
        console.error(
          `${engine.id} ${page.verdict.padEnd(7)} ${String(page.status ?? "-").padEnd(3)} ${String(page.ms).padStart(5)} ms  ${site.url}${page.reason ? `  (${page.reason})` : ""}`,
        );
      });
      if (engine.role === "full")
        for (const name of Object.keys(manifest.front))
          for (const [i, site] of sites.filter((s) => s.from === "seed").entries()) {
            const result = await front(name, running, site.url, dir, i);
            info.fronts.push(result);
            store.emit("front", { run: row.id, engine: engine.id, ...result });
            console.error(
              `${engine.id} ${name} ${result.ok ? "ok" : result.skip ? "SKIP" : "FAIL"}  ${site.url}  (${result.detail})`,
            );
          }
      Object.assign(info, { state: "done", peak_rss_mib: running.peak(), features: (await running.features) ?? {} });
      store.saveEngine(row.id, engine.id, info);
      store.emit("engine", { run: row.id, ...info });
    } finally {
      await running.stop();
      current = null;
    }
  }
  const rep = report(store, row.id)!;
  const taught = store.learn(
    row.id,
    lessonsOf(rep),
    rep.engines.flatMap((e) =>
      e.pages.flatMap((p, i) => (p ? [{ host: new URL(rep.corpus[i].url).host, engine: e.id }] : [])),
    ),
  );
  store.emit("lessons", { run: row.id, ...taught });
  store.finishRun(row.id, rep.ok);
  rep.finished = new Date().toISOString();
  await Bun.write(join(out, "report.json"), JSON.stringify(rep, null, 1));
  await Bun.write(join(out, "report.md"), markdown(rep));
  store.emit("run", { id: row.id, state: "finished", ok: rep.ok });
  const base = resolve(root, "out/acceptance");
  for (const old of store
    .runs(1000)
    .filter((r) => r.host === host)
    .slice(manifest.store.keep_runs))
    if (resolve(old.meta.out).startsWith(base + (process.platform === "win32" ? "\\" : "/"))) await rmrf(old.meta.out);
  for (const e of rep.engines)
    console.log(
      `${e.id} ${e.version}: ${Object.entries(e.counts)
        .map(([k, n]) => `${n} ${k}`)
        .join(", ")}; median load ${e.median_ms} ms, peak RSS ${e.peak_rss_mib} MiB`,
    );
  if (rep.parity.length)
    console.log(`lite is enough for ${rep.parity.filter((p) => p.sufficient).length} of ${rep.parity.length} pages`);
  for (const line of rep.regressions) console.log(`regression: ${line}`);
  console.log(`${rep.ok ? "ok" : "FAILED"}: ${join(out, "report.md")}`);
  store.close();
  process.exit(rep.ok ? 0 : 1);
}

// ---- benchmarks ---------------------------------------------------------------------------------------------

export type Timing = {
  command: string;
  mean: number;
  stddev: number | null;
  median: number;
  user: number;
  system: number;
  min: number;
  max: number;
  times: number[];
  exit_codes: (number | null)[];
  memory_usage_byte?: number[];
};

type Metric = { value: number };
type V2Result = {
  command: string;
  measurements: Record<string, Metric | number>[];
  summary: Record<string, { mean: number; stddev?: number; median: number; min: number; max: number }>;
};

/** hyperfine 1.x exports mean/times per result; 2.x exports measurements and a summary per metric. Both give a Timing. */
export function readHyperfine(json: { results: unknown[]; schema_version?: number }): Timing[] {
  if (json.schema_version !== 2) return json.results as Timing[];
  return (json.results as V2Result[]).map((r) => {
    const value = (m: Record<string, Metric | number>, key: string) => {
      const v = m[key];
      return typeof v === "object" ? v.value : null;
    };
    const wall = r.summary.time_wall_clock;
    const memory = r.measurements
      .map((m) => value(m, "memory_usage") ?? value(m, "memory_usage_byte"))
      .filter((v): v is number => v !== null);
    return {
      command: r.command,
      mean: wall.mean,
      stddev: wall.stddev ?? null,
      median: wall.median,
      user: r.summary.time_user?.mean ?? 0,
      system: r.summary.time_system?.mean ?? 0,
      min: wall.min,
      max: wall.max,
      times: r.measurements.map((m) => value(m, "time_wall_clock") ?? 0),
      exit_codes: r.measurements.map((m) => (typeof m.exit_code === "number" ? m.exit_code : null)),
      ...(memory.length && { memory_usage_byte: memory }),
    };
  });
}

/** hyperfine flags observations beyond 14.826 median absolute deviations; the same rule, on the stored times. */
export function outliers(times: number[]): number {
  const mid = median(times)!;
  const mad = median(times.map((t) => Math.abs(t - mid)))!;
  return mad ? times.filter((t) => Math.abs(t - mid) / (1.4826 * mad) > 10).length : 0;
}

/** One-shot run of a command with its process tree sampled every 5 ms: wall time and peak resident memory. */
async function sampled(argv: string[]): Promise<{ wall_s: number; rss_mib: number | null; code: number }> {
  const t0 = performance.now();
  const proc = spawn(argv, { stdin: "ignore", stdout: "ignore", stderr: "ignore" });
  const rss = sampleRss(proc.pid, 5);
  const code = await proc.exited;
  rss.stop();
  const peak = rss.peak();
  return {
    wall_s: (performance.now() - t0) / 1000,
    rss_mib: peak === null ? null : Math.round((peak / 1024) * 10) / 10,
    code,
  };
}

async function versionOf(engine: Engine): Promise<string | null> {
  const running = await launch(engine).catch(() => null);
  await running?.stop();
  return running?.version ?? null;
}

/** The JetStream commit pinned in tools/upstreams.lock.json, checked out once under the baseline directory. */
async function jetstreamCheckout(): Promise<string> {
  const lock = (await Bun.file(join(root, "tools/upstreams.lock.json")).json()) as Record<
    string,
    { sha: string; url: string }
  >;
  const pin = lock.JetStream;
  if (!pin) throw new Error("JetStream is not pinned in tools/upstreams.lock.json (import.py pin mode)");
  const dir = join(baselineDirs()[0], `JetStream-${pin.sha.slice(0, 12)}`);
  if (await exists(join(dir, "JetStreamDriver.js"))) return dir;
  await mkdirp(dir);
  for (const args of [
    ["init", "-q"],
    ["remote", "add", "origin", pin.url],
    ["fetch", "-q", "--depth", "1", "origin", pin.sha],
    ["checkout", "-q", "FETCH_HEAD"],
  ]) {
    const code = await spawn(["git", ...args], { cwd: dir, stdin: "ignore", stdout: "ignore", stderr: "inherit" })
      .exited;
    if (code !== 0) throw new Error(`git ${args[0]} failed in ${dir}`);
  }
  return dir;
}

async function bench(urls: string[], flag: Flags) {
  const store = await openStore(flag);
  const found = await engines(flag.engine, flag.all);
  const env = {
    host: hostname(),
    cores: cores(),
    platform: `${process.platform} ${process.arch}`,
    load: loadavg().map((l) => Math.round(l * 100) / 100),
  };
  if (flag.jetstream) {
    const { jetstream } = await import("./acceptance/jetstream.ts");
    const dir = await jetstreamCheckout();
    for (const engine of found.filter((e) => e.role === "full")) {
      const running = await launch(engine);
      try {
        const result = await jetstream(running.ws, dir, {
          tests: flag.test,
          onProgress: (line) => console.error(`${engine.id} ${line}`),
        });
        const row = { ...result, peak_rss_mib: running.peak(), env };
        store.saveBench({
          host: env.host,
          suite: "jetstream",
          engine: engine.id,
          version: running.version,
          target: `JetStream@${result.revision?.slice(0, 12)}${flag.test?.length ? ` (${flag.test.join(",")})` : ""}`,
          result: row,
        });
        store.emit("bench", {
          suite: "jetstream",
          engine: engine.id,
          version: running.version,
          score: result.score,
          error: result.error,
        });
        console.log(
          `${engine.id} ${running.version}: JetStream ${result.error ? `failed: ${result.error}` : result.score} in ${Math.round(result.ms / 1000)} s`,
        );
      } finally {
        await running.stop();
      }
    }
    return store.close();
  }
  const hyperfine = await tool("hyperfine");
  if (!hyperfine)
    throw new Error(
      "hyperfine not found: build it on the build host (cargo build --release -p hyperfine) or put it on PATH",
    );
  const [warmup, runs] = [Number(flag.warmup ?? manifest.bench.warmup), Number(flag.runs ?? manifest.bench.runs)];
  const timed = found.filter((e) => e.once);
  const versions = new Map<string, string | null>();
  for (const engine of timed) versions.set(engine.id, await versionOf(engine));
  // hyperfine -N splits commands like a shell would: quote what needs it, and keep backslashes out of the way.
  const quote = (arg: string) =>
    /[\s"']/.test(arg) ? `"${arg.replaceAll("\\", "/").replaceAll('"', '\\"')}"` : arg.replaceAll("\\", "/");
  for (const url of urls.length ? urls : manifest.corpus.seeds) {
    const work = await mkdtemp("acceptance-bench-");
    const commands = timed.map((engine) => [
      engine.path,
      ...engine.once!.map((a) => fill(a, { url, profile: join(work, "profiles", engine.id) })),
    ]);
    const json = join(work, "hyperfine.json");
    const wipe = [process.execPath, import.meta.path, "wipe", join(work, "profiles")].map(quote).join(" ");
    const argv = [
      hyperfine,
      "-N",
      "--warmup",
      String(warmup),
      "--runs",
      String(runs),
      "--ignore-failure",
      "--prepare",
      wipe,
      "--export-json",
      json,
    ];
    for (const [i, engine] of timed.entries()) argv.push("--command-name", engine.id, commands[i].map(quote).join(" "));
    console.error(
      `bench ${url}: ${timed.map((e) => e.id).join(" vs ")}, ${warmup} warm-ups + ${runs} runs each, load ${env.load.join(" ")}`,
    );
    const code = await spawn(argv, { stdin: "ignore", stdout: "inherit", stderr: "inherit" }).exited;
    if (code !== 0) throw new Error(`hyperfine exited with ${code}`);
    const results = readHyperfine(await Bun.file(json).json());
    const fastest = results.reduce((a, b) => (a.mean <= b.mean ? a : b));
    for (const [i, engine] of timed.entries()) {
      const t = results[i];
      const rss: number[] = [];
      for (let n = 0; n < manifest.bench.rss_runs; n++) {
        await rmrf(join(work, "profiles"));
        const one = await sampled(commands[i]);
        if (one.code === 0 && one.rss_mib !== null) rss.push(one.rss_mib);
      }
      const ratio = t.mean / fastest.mean;
      const result = {
        mean_s: t.mean,
        stddev_s: t.stddev,
        median_s: t.median,
        min_s: t.min,
        max_s: t.max,
        user_s: t.user,
        system_s: t.system,
        runs: t.times.length,
        warmup,
        failed: t.exit_codes.filter((c) => c !== 0).length,
        outliers: outliers(t.times),
        relative: ratio,
        relative_stddev: ratio * Math.hypot((t.stddev ?? 0) / t.mean, (fastest.stddev ?? 0) / fastest.mean),
        max_rss_mib: t.memory_usage_byte?.length ? Math.round(Math.max(...t.memory_usage_byte) / 1048576) : null,
        tree_rss_mib: median(rss),
        tree_rss_runs: rss.length,
        times: t.times,
        command: commands[i].join(" "),
        env,
      };
      store.saveBench({
        host: env.host,
        suite: "cold-start",
        engine: engine.id,
        version: versions.get(engine.id) ?? null,
        target: url,
        result,
      });
      store.emit("bench", {
        suite: "cold-start",
        engine: engine.id,
        target: url,
        mean_s: t.mean,
        stddev_s: t.stddev,
        tree_rss_mib: result.tree_rss_mib,
      });
      console.log(
        `${engine.id.padEnd(8)} ${(t.mean * 1000).toFixed(1)} ms ± ${((t.stddev ?? 0) * 1000).toFixed(1)}  [${(t.min * 1000).toFixed(1)} … ${(t.max * 1000).toFixed(1)}]  ` +
          `${ratio.toFixed(2)} ± ${result.relative_stddev.toFixed(2)}x  tree RSS ${result.tree_rss_mib ?? "-"} MiB  ${result.failed ? `${result.failed} failed  ` : ""}${result.outliers ? `${result.outliers} outliers  ` : ""}${url}`,
      );
    }
    await rmrf(work);
  }
  store.close();
}

// ---- other commands -----------------------------------------------------------------------------------------

async function track(flag: Flags) {
  const store = await openStore(flag);
  const { track: follow } = await import("./acceptance/track.ts");
  for (const row of await follow(root)) {
    const moved = store.saveUpstream(row.name, row);
    if (moved) store.emit("upstream", row);
    const state = row.error
      ? `ERROR ${row.error}`
      : `${row.version ?? "-"}  head ${row.head?.slice(0, 12) ?? "-"} ${row.head_date?.slice(0, 10) ?? ""}  ${row.behind === null ? "" : `pin ${row.behind} behind`}`;
    console.log(`${moved ? "*" : " "} ${row.name.padEnd(20)} ${row.kind.padEnd(7)} ${state}`);
  }
  store.close();
}

async function cookies(endpoint: string, file: string, flag: Flags) {
  const ws = endpoint.startsWith("ws")
    ? endpoint
    : ((await (await fetch(new URL("/json/version", endpoint))).json()) as any).webSocketDebuggerUrl;
  const cdp = await Cdp.open(ws);
  const { cookies: jar } = await cdp.send<{ cookies: Record<string, any>[] }>("Storage.getCookies");
  cdp.close();
  const store = await openStore(flag);
  const hosts = [...new Set((await corpus(store, manifest.corpus)).map((s) => s.host))];
  store.close();
  const fields = [
    "name",
    "value",
    "domain",
    "path",
    "secure",
    "httpOnly",
    "sameSite",
    "priority",
    "sourceScheme",
    "sourcePort",
    "partitionKey",
  ];
  const kept = jar
    .filter((c) =>
      hosts.some((h) => h === c.domain.replace(/^\./, "") || h.endsWith(`.${c.domain.replace(/^\./, "")}`)),
    )
    .map((c) => ({
      ...Object.fromEntries(fields.filter((f) => c[f] !== undefined).map((f) => [f, c[f]])),
      ...(c.expires > 0 && { expires: c.expires }),
    }));
  await Bun.write(file, JSON.stringify(kept), { mode: 0o600 });
  console.log(
    `${kept.length} cookies for the ${hosts.length} corpus hosts -> ${file} (credentials: keep it out of git)`,
  );
}

type Flags = ReturnType<typeof flags>["values"];
const flags = () =>
  parseArgs({
    args: Bun.argv.slice(2),
    allowPositionals: true,
    options: {
      engine: { type: "string", multiple: true },
      site: { type: "string", multiple: true },
      test: { type: "string", multiple: true },
      all: { type: "boolean" },
      fresh: { type: "boolean" },
      install: { type: "boolean" },
      "no-discover": { type: "boolean" },
      jetstream: { type: "boolean" },
      apply: { type: "boolean" },
      schedule: { type: "boolean" },
      json: { type: "boolean" },
      jobs: { type: "string" },
      out: { type: "string" },
      cookies: { type: "string" },
      profile: { type: "string" },
      db: { type: "string" },
      redis: { type: "string" },
      warmup: { type: "string" },
      runs: { type: "string" },
      port: { type: "string" },
    },
  });

if (import.meta.main) {
  const {
    values: flag,
    positionals: [command, ...args],
  } = flags();
  if (command === "engines") {
    for (const engine of await engines(flag.engine, true)) {
      const version = await versionOf(engine);
      console.log(
        `${engine.id.padEnd(9)} ${engine.role.padEnd(5)} ${engine.origin.padEnd(8)} ${version ?? "BROKEN (opens no CDP endpoint)"}  ${engine.path}`,
      );
    }
    for (const [name, def] of Object.entries(manifest.front))
      console.log(
        `${name.padEnd(9)} front ${def.bins.length ? ((await Promise.all(def.bins.map(tool))).find(Boolean) ?? "MISSING") : `Bun.WebView ${typeof Bun.WebView === "function" ? Bun.version : "MISSING"}`}`,
      );
    console.log(`hyperfine tool  ${(await tool("hyperfine")) ?? "MISSING"}`);
  } else if (command === "install") await install(flag.engine);
  else if (command === "sites") {
    const store = await openStore(flag);
    for (const site of await corpus(store, manifest.corpus, { discover: !flag["no-discover"], fresh: flag.fresh }))
      console.log(`${site.url}\t${site.from}`);
    store.close();
  } else if (command === "run") await run(flag);
  else if (command === "bench") await bench(args, flag);
  else if (command === "track") await track(flag);
  else if (command === "lessons") {
    const store = await openStore(flag);
    const rows = store.lessons();
    console.log(
      flag.json
        ? JSON.stringify(rows, null, 1)
        : rows
            .map(
              (l) =>
                `${l.host.padEnd(36)} ${l.engine.padEnd(8)} ${l.kind.padEnd(16)} ${String(l.runs).padStart(3)} runs  ${l.detail}`,
            )
            .join("\n") || "nothing learned yet",
    );
    store.close();
  } else if (command === "serve")
    await (
      await import("./acceptance/serve.ts")
    ).serve(await openStore(flag), { port: Number(flag.port ?? manifest.serve.port), schedule: !!flag.schedule });
  else if (command === "deploy") process.exit(await (await import("./acceptance/deploy.ts")).deploy(!!flag.apply));
  else if (command === "report") {
    const store = await openStore(flag);
    const rep = report(store, args[0]);
    console.log(rep ? (flag.json ? JSON.stringify(rep, null, 1) : markdown(rep)) : "no run recorded");
    store.close();
  } else if (command === "cookies" && args[1]) await cookies(args[0], args[1], flag);
  else if (command === "webview" && args[2]) await webview(args);
  else if (command === "wipe" && args[0] && resolve(args[0]).startsWith(resolve(tmpdir()))) await rmrf(args[0]);
  else {
    console.error(
      (await Bun.file(import.meta.path).text())
        .split("\n")
        .slice(1, 24)
        .map((line) => line.replace(/^\/\/ ?/, ""))
        .join("\n"),
    );
    process.exit(2);
  }
}
