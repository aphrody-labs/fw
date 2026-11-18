import { join } from "node:path";
import {
  ApiError,
  CHANNELS,
  CHANNELS_DOC_SOURCE,
  CHANNELS_PAGE,
  DASH_CHANNELS,
  DASH_ENDPOINTS,
  DASH_ORIGIN,
  DASH_PLATFORMS,
  HttpError,
  STATUS_ENDPOINTS,
  STATUS_ORIGIN,
  fetchChannelsPage,
  hostPlatform,
  loadCefIndex,
  measuredStableWeeks,
  pickChromium,
  withReleases,
  type CefPlatform,
  type Clients,
  type DashChannel,
  type DashPlatform,
  type Entry,
} from "@aphrody/ay-chromium-releases";
import { layout } from "./layout.ts";
import * as v from "./views.ts";
import { html, type Safe } from "./html.ts";

export interface AppOptions {
  clients: Clients;
  /** The repository root, where `brand/` lives. */
  root?: string;
  /** Product name in the page title. */
  product?: string;
  fetch?: typeof fetch;
}

const PLATFORMS: DashPlatform[] = ["Windows", "Mac", "Linux", "Android", "iOS", "Webview"];
const DASH_JSON = new Set<string>(DASH_ENDPOINTS.filter(e => !e.auth && e.method === "GET" && !e.path.endsWith("csv")).map(e => e.path));
const STATUS_JSON = STATUS_ENDPOINTS.filter(e => !e.auth).map(e => new RegExp(`^${e.path.replace(/\{[^}]+\}/g, "[^/]+")}$`));
const HEX = /^[0-9a-fA-F]{1,40}$/;

const ASSETS: Record<string, [string, string]> = {
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

class NotFound extends Error {}

export function createApp(options: AppOptions) {
  const { clients } = options;
  const root = options.root ?? join(import.meta.dir, "../../..");
  const product = options.product ?? "Chromium Dash";
  const { dash, status, source } = clients;

  /** Track whether any document of the page came from a stale cache entry. */
  const seen = { stale: false };
  const take = <T>(e: Entry<T>): T => {
    if (e.stale) seen.stale = true;
    return e.value;
  };

  const json = (value: unknown, init: ResponseInit & { stale?: boolean } = {}) =>
    new Response(JSON.stringify(value), { ...init, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "public, max-age=60", ...(init.stale ? { "x-cache-stale": "1" } : {}), ...init.headers } });

  const page = (p: { title: string; path: string; subtitle?: string; body: Safe }) => new Response(layout({ ...p, product, stale: seen.stale }), { headers: { "content-type": "text/html; charset=utf-8" } });

  const platformOf = (u: URL): DashPlatform => {
    const p = u.searchParams.get("platform") ?? "Windows";
    return (DASH_PLATFORMS as readonly string[]).includes(p) ? (p as DashPlatform) : "Windows";
  };
  const int = (s: string | null, d: number, max = 1000) => Math.min(Math.max(Number.parseInt(s ?? "", 10) || d, 0), max);

  async function platformTabs(u: URL, path: string) {
    const current = platformOf(u);
    return v.tabs(PLATFORMS.map(p => ({ href: `${path}?platform=${p}`, label: p, current: p === current })));
  }

  async function newestRows(platform: DashPlatform, channels: readonly DashChannel[] = ["Stable", "Beta", "Dev", "Canary"]) {
    return Promise.all(channels.map(async channel => ({ channel, release: take(await dash.releases({ channel, platform, num: 1 }))[0] })));
  }

  async function routePage(u: URL): Promise<Response> {
    const path = u.pathname.replace(/\/+$/, "") || "/";
    if (path === "/" || path === "/home") {
      const platform = platformOf(u);
      const [rows, milestones, schedule, all] = await Promise.all([
        newestRows(platform),
        dash.milestones({ onlyActive: true }).then(take),
        dash.schedule({ mstone: "current", offset: -1, n: 3 }).then(take),
        newestRows(platform, DASH_CHANNELS.filter(c => c !== "Canary_asan")),
      ]);
      const releases = all.flatMap(r => (r.release ? [r.release] : []));
      const measured = measuredStableWeeks((await dash.schedule({ offset: -4, n: 12 }).then(take)).mstones);
      return page({
        title: "Home",
        path: "/",
        body: html`${await platformTabs(u, "/")}${v.home({ platform, channelRows: rows, milestones: milestones.sort((a, b) => b.milestone - a.milestone), schedule: schedule.mstones, channels: withReleases(releases, platform), measuredWeeks: measured })}`,
      });
    }
    if (path === "/releases") {
      const platform = platformOf(u);
      const channelParam = u.searchParams.get("channel");
      const num = int(u.searchParams.get("num"), 6, 100);
      const channels = channelParam && (DASH_CHANNELS as readonly string[]).includes(channelParam) ? [channelParam as DashChannel] : DASH_CHANNELS.filter(c => c !== "Canary_asan");
      const cards = await Promise.all(
        channels.map(async channel => {
          const rows = take(await dash.releases({ channel, platform, num }));
          const more = rows.length >= num ? `/releases?${new URLSearchParams({ platform, channel, num: String(num + 10) })}` : undefined;
          return v.releaseCard(channel, rows, more);
        }),
      );
      return page({ title: "Releases", subtitle: platform, path, body: html`${await platformTabs(u, "/releases")}<div class="cards">${cards}</div>` });
    }
    if (path === "/schedule") {
      const from = u.searchParams.get("from");
      const start = from && /^\d+$/.test(from) ? Number(from) : undefined;
      const rows = take(await (start ? dash.schedule({ mstone: start, n: 3 }) : dash.schedule({ mstone: "current", offset: -1, n: 3 }))).mstones;
      const first = rows[0]?.mstone ?? start ?? 0;
      return page({ title: "Schedule", path, body: v.schedule(rows, `/schedule?from=${Math.max(first - 3, 1)}`, `/schedule?from=${first + 3}`) });
    }
    if (path === "/branches") {
      const [ms, mini] = await Promise.all([dash.milestones().then(take), dash.minibranches().then(take)]);
      return page({ title: "Branches", path, body: v.branches(ms.sort((a, b) => b.milestone - a.milestone).slice(0, 40), mini) });
    }
    if (path === "/commits" || path.startsWith("/commit/")) {
      const q = (path.startsWith("/commit/") ? path.slice("/commit/".length) : (u.searchParams.get("q") ?? "")).trim();
      const recent = take(await dash.commits({ count: 25 })).commits.slice(0, 25);
      if (!q) return page({ title: "Commits", path: "/commits", body: v.commitsPage("", recent) });
      if (!HEX.test(q)) return page({ title: "Commits", path: "/commits", body: v.commitsPage(q, recent, undefined, "A commit hash is 1 to 40 hexadecimal digits.") });
      try {
        const found = take(await dash.commit(q));
        return page({ title: "Commit", subtitle: q.slice(0, 12), path: "/commits", body: v.commitsPage(q, recent, found) });
      } catch (e) {
        if (e instanceof ApiError || e instanceof HttpError) return page({ title: "Commits", path: "/commits", body: v.commitsPage(q, recent, undefined, `No commit ${q}: ${e instanceof ApiError ? e.detail : e.status}`) });
        throw e;
      }
    }
    if (path === "/serving-builds") return page({ title: "Serving builds", path, body: v.servingBuilds(take(await dash.crosServingBuilds())) });
    if (path === "/channels") {
      const platform = platformOf(u);
      const [rows, sched, pageData] = await Promise.all([newestRows(platform, DASH_CHANNELS.filter(c => c !== "Canary_asan")), dash.schedule({ offset: -4, n: 12 }).then(take), fetchChannelsPage(options.fetch).catch(() => undefined)]);
      return page({
        title: "Release channels",
        subtitle: platform,
        path,
        body: html`${await platformTabs(u, "/channels")}${v.channelsPage({ channels: withReleases(rows.flatMap(r => (r.release ? [r.release] : [])), platform), page: pageData, measuredWeeks: measuredStableWeeks(sched.mstones), platform, source: { page: CHANNELS_PAGE, doc: CHANNELS_DOC_SOURCE } })}`,
      });
    }
    if (path === "/roadmap") {
      const channels = take(await status.channels());
      const picks = [["stable", "Stable"], ["beta", "Next up"], ["dev", "Dev"]] as const;
      const cols = await Promise.all(picks.filter(([k]) => channels[k]).map(async ([k, tag]) => ({ tag, sched: channels[k]!, features: take(await status.inMilestone(channels[k]!.mstone)) })));
      return page({ title: "Roadmap", path, subtitle: "Chrome Platform Status", body: v.roadmap(cols) });
    }
    if (path === "/features") {
      const q = u.searchParams.get("q") ?? "";
      const sort = u.searchParams.get("sort") ?? "";
      const start = int(u.searchParams.get("start"), 0, 100_000);
      const num = int(u.searchParams.get("num"), 25, 100) || 25;
      const list = take(await status.search({ q: q || undefined, sort: sort || undefined, start, num }));
      return page({ title: "All features", path, subtitle: "Chrome Platform Status", body: v.featuresPage({ q, sort, start, num, list }) });
    }
    const m = /^\/feature\/(\d+)$/.exec(path);
    if (m) {
      const id = Number(m[1]);
      try {
        const [f, gates] = await Promise.all([status.feature(id).then(take), status.gates(id).then(take).catch(() => ({ gates: [] }))]);
        return page({ title: "Feature", path: "/features", subtitle: "Chrome Platform Status", body: v.featurePage(f, gates.gates) });
      } catch (e) {
        if (e instanceof HttpError && e.status === 404) throw new NotFound(`feature ${id}`);
        throw e;
      }
    }
    if (path === "/api") return page({ title: "API", path, body: v.apiPage(u.origin, DASH_ENDPOINTS, STATUS_ENDPOINTS) });
    throw new NotFound(path);
  }

  async function routeApi(u: URL): Promise<Response | undefined> {
    const { pathname, search } = u;
    if (pathname === "/healthz") return json({ ok: true });
    if (pathname === "/api/channels") {
      const platform = platformOf(u);
      const rows = (await Promise.all(CHANNELS.map(c => dash.releases({ channel: c.dash, platform, num: 1 }).then(take)))).flat();
      const measured = measuredStableWeeks((await dash.schedule({ offset: -4, n: 12 }).then(take)).mstones);
      return json({ platform, channels: withReleases(rows, platform), stableMajorWeeksMeasured: measured, stale: seen.stale });
    }
    if (pathname === "/api/pick") {
      const cef = (u.searchParams.get("platform") ?? undefined) as CefPlatform | undefined;
      const sel = await pickChromium(dash, () => loadCefIndex({ cache: clients.cache, fetch: options.fetch }), { channel: u.searchParams.get("channel") ?? "stable", cefPlatform: cef ?? hostPlatform().cef });
      return json(sel);
    }
    if (DASH_JSON.has(pathname)) {
      const entry = await source.get<unknown>(`${DASH_ORIGIN}${pathname}${search}`, pathname === "/fetch_commit" ? 30 * 24 * 3_600_000 : pathname.includes("releases") || pathname === "/fetch_commits" ? 15 * 60_000 : 6 * 3_600_000);
      return json(entry.value, { stale: entry.stale });
    }
    if (pathname.startsWith("/api/v0/")) {
      const sub = pathname.slice("/api/v0".length);
      if (!STATUS_JSON.some(re => re.test(sub))) return json({ error: "not served" }, { status: 404 });
      const entry = await source.get<unknown>(`${STATUS_ORIGIN}${pathname}${search}`, sub.startsWith("/features") ? 30 * 60_000 : 6 * 3_600_000);
      return new Response(`)]}'\n${JSON.stringify(entry.value)}`, { headers: { "content-type": "application/json; charset=utf-8", "cache-control": "public, max-age=60", ...(entry.stale ? { "x-cache-stale": "1" } : {}) } });
    }
    return undefined;
  }

  async function asset(pathname: string): Promise<Response | undefined> {
    const hit = ASSETS[pathname];
    if (!hit) return undefined;
    const file = Bun.file(join(root, hit[0]));
    if (!(await file.exists())) return undefined;
    return new Response(file, { headers: { "content-type": hit[1], "cache-control": "public, max-age=3600" } });
  }

  return {
    async fetch(req: Request): Promise<Response> {
      const u = new URL(req.url);
      if (req.method !== "GET" && req.method !== "HEAD") return new Response("method not allowed", { status: 405 });
      seen.stale = false;
      try {
        return (await asset(u.pathname)) ?? (await routeApi(u)) ?? (await routePage(u));
      } catch (e) {
        const wantsJson = u.pathname.startsWith("/api/") || u.pathname.startsWith("/fetch_") || u.pathname.startsWith("/cros/");
        if (e instanceof NotFound) return wantsJson ? json({ error: `not found: ${e.message}` }, { status: 404 }) : new Response(layout({ title: "Not found", path: "", product, body: html`<p class="lead">Nothing at ${e.message}.</p>` }), { status: 404, headers: { "content-type": "text/html; charset=utf-8" } });
        if (e instanceof ApiError) return json({ error: e.detail }, { status: 400 });
        const status = e instanceof HttpError ? 502 : 500;
        const message = e instanceof Error ? e.message : String(e);
        return wantsJson ? json({ error: message }, { status }) : new Response(layout({ title: "Error", path: "", product, body: html`<p class="stale">${message}</p>` }), { status, headers: { "content-type": "text/html; charset=utf-8" } });
      }
    },
  };
}
