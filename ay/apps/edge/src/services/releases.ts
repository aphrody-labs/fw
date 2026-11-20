// Downloads and releases. Layout and manifests: ../manifest.ts. Hosts and routes: edge.toml [services.releases].
//   /                                    portal: every product, its channels and the files for the visitor's platform
//   /manifest.json                       index of every product and channel (also /api/releases)
//   /api/releases/<product>/<channel>/latest   release manifest of the channel pointer, with absolute asset urls
//   /api/releases/<product>/<version>    release manifest of one version
//   /<product>/latest/<os>-<arch>        302 to the newest asset for that platform (?channel= ?kind= ?abi=)
//   /<product>/<version>/<file>          the object (asset, SHA256SUMS, release.json), ranges and ETags
//   /brand/<file>                        identity files of the assets bucket (icon.svg, favicon, apple-touch-icon)
//   /ay-tools/<file>                     Windows builds of ay-tools (bucket CHROMIUM)
// Legacy aphrody layout (channels/, manifests/, scripts/, <channel>/<version>/<asset>) stays readable from the same bucket.
import toolsRelease from "../../../tools-release/src/index.ts";
import {
  CHANNELS,
  type ChannelPointer,
  type IndexManifest,
  type ReleaseManifest,
  SCHEMA_INDEX,
  isChannel,
  isSegment,
  pickAsset,
  releaseKeys,
} from "../manifest.ts";
import { IMMUTABLE, NOT_FOUND, REVALIDATE, serveObject } from "../r2.ts";
import type { Routed, Service } from "../types.ts";

const SHORT = "public, max-age=60";
const json = (body: unknown, cache = SHORT, status = 200): Response =>
  Response.json(body, { status, headers: { "cache-control": cache, "access-control-allow-origin": "*" } });

const esc = (s: string): string => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

async function readJson<T>(r: Routed, key: string): Promise<T | null> {
  const obj = await r.env.RELEASES.get(key);
  if (!obj) return null;
  try {
    return (await obj.json()) as T;
  } catch {
    return null;
  }
}

export const readIndex = async (r: Routed): Promise<IndexManifest> =>
  (await readJson<IndexManifest>(r, releaseKeys.index())) ?? { schema: SCHEMA_INDEX, generated: new Date(0).toISOString(), products: {} };

/** A release manifest with an absolute url on every asset, from the origin the visitor used. */
function withUrls(m: ReleaseManifest, origin: string): ReleaseManifest & { assets: Array<ReleaseManifest["assets"][number] & { url: string }> } {
  return { ...m, assets: m.assets.map((a) => ({ ...a, url: `${origin}/${m.product}/${m.version}/${a.name}` })) };
}

/** Platform of a User-Agent, in the vocabulary of ay.toml [sdk]. */
export function detectPlatform(ua: string): { os: string; arch: string } {
  const arm = /arm64|aarch64|\bARM\b|Apple Silicon/i.test(ua);
  if (/Windows/i.test(ua)) return { os: "windows", arch: arm ? "aarch64" : "x86_64" };
  if (/Android/i.test(ua)) return { os: "android", arch: arm ? "aarch64" : "x86_64" };
  if (/Macintosh|Mac OS X/i.test(ua)) return { os: "macos", arch: /Intel/i.test(ua) && !arm ? "x86_64" : "aarch64" };
  if (/Linux|X11/i.test(ua)) return { os: "linux", arch: arm ? "aarch64" : "x86_64" };
  return { os: "any", arch: "any" };
}

async function latestManifest(r: Routed, product: string, channel: string): Promise<ReleaseManifest | null> {
  if (!isSegment(product) || !isChannel(channel)) return null;
  const pointer = await readJson<ChannelPointer>(r, releaseKeys.channel(product, channel));
  if (!pointer) return null;
  return readJson<ReleaseManifest>(r, releaseKeys.release(product, pointer.version));
}

async function portal(r: Routed): Promise<Response> {
  const index = await readIndex(r);
  const wanted = r.url.searchParams.get("product");
  const channelParam = (r.url.searchParams.get("channel") ?? "stable").toLowerCase();
  const channel = isChannel(channelParam) ? channelParam : "stable";
  const me = detectPlatform(r.request.headers.get("user-agent") ?? "");
  const origin = r.origin;
  const rows: string[] = [];
  for (const product of Object.keys(index.products).sort()) {
    if (wanted && wanted !== product) continue;
    const pointer = index.products[product]?.[channel];
    if (!pointer) {
      rows.push(`<tr><th>${esc(product)}</th><td colspan="4">no ${esc(channel)} release yet</td></tr>`);
      continue;
    }
    const manifest = await readJson<ReleaseManifest>(r, releaseKeys.release(product, pointer.version));
    if (!manifest) continue;
    const mine = pickAsset(manifest, me.os, me.arch)?.name;
    for (const a of manifest.assets) {
      const href = `${origin}/${product}/${manifest.version}/${a.name}`;
      rows.push(
        `<tr${a.name === mine ? ' class="mine"' : ""}><th>${esc(product)}</th><td>${esc(manifest.version)}</td><td><a href="${esc(href)}">${esc(a.name)}</a></td><td>${esc(a.os)}/${esc(a.arch)}${a.abi ? `/${esc(a.abi)}` : ""}</td><td><code>${esc(a.sha256.slice(0, 16))}</code></td></tr>`,
      );
    }
  }
  if (!rows.some((x) => x.includes("<a href"))) {
    // The aphrody pipeline published before the manifest layout existed: channels/<channel>/latest.json names one asset under /assets/.
    const legacy = await readJson<{ product?: string; version?: string; os?: string; arch?: string; asset?: string; sha256?: string }>(r, `channels/${channel}/latest.json`);
    if (legacy?.asset && isSegment(legacy.asset)) {
      const href = `${origin}/assets/${channel}/latest/${legacy.asset}`;
      rows.length = 0;
      rows.push(`<tr><th>${esc(legacy.product ?? "aphrody")}</th><td>${esc(legacy.version ?? "")}</td><td><a href="${esc(href)}">${esc(legacy.asset)}</a></td><td>${esc(legacy.os ?? "")}/${esc(legacy.arch ?? "")}</td><td><code>${esc((legacy.sha256 ?? "").slice(0, 16))}</code></td></tr>`);
    }
  }
  const channels = CHANNELS.map((c) => `<a href="?channel=${c}"${c === channel ? ' aria-current="page"' : ""}>${c}</a>`).join(" ");
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Downloads</title>
<link rel="icon" href="${r.origin}/brand/icon.svg" type="image/svg+xml"><link rel="apple-touch-icon" href="${r.origin}/brand/apple-touch-icon.png">
<style>:root{color-scheme:light dark}body{font:16px/1.5 system-ui,sans-serif;margin:0 auto;max-width:60rem;padding:2rem 1rem}
table{border-collapse:collapse;width:100%}th,td{text-align:left;padding:.4rem .6rem;border-bottom:1px solid #8884}tr.mine{background:#8f8a}
nav a{margin-right:.8rem}a[aria-current]{font-weight:700}code{font-size:.85em}</style></head>
<body><h1>Downloads</h1><nav>${channels}</nav>
<p>Your platform: <b>${esc(me.os)}/${esc(me.arch)}</b>. Index: <a href="${origin}/manifest.json">manifest.json</a>.</p>
<table><thead><tr><th>Product</th><th>Version</th><th>File</th><th>Target</th><th>SHA-256</th></tr></thead><tbody>${rows.join("") || '<tr><td colspan="5">No release published yet.</td></tr>'}</tbody></table>
</body></html>`;
  return new Response(html, { headers: { "content-type": "text/html; charset=utf-8", "cache-control": SHORT } });
}

/** /<product>/latest/<os>-<arch> -> 302 to the asset. */
async function latestRedirect(r: Routed, product: string, target: string): Promise<Response> {
  const dash = target.indexOf("-");
  const os = dash < 0 ? target : target.slice(0, dash);
  const arch = dash < 0 ? "any" : target.slice(dash + 1);
  const channel = (r.url.searchParams.get("channel") ?? "stable").toLowerCase();
  const manifest = await latestManifest(r, product, channel);
  if (!manifest) return json({ error: "no release", product, channel }, "no-store", 404);
  const kind = r.url.searchParams.get("kind") ?? undefined;
  const abi = r.url.searchParams.get("abi") ?? undefined;
  const asset = pickAsset(manifest, os, arch, { kind: kind as never, abi });
  if (!asset) return json({ error: "no asset for target", product, channel, os, arch, available: manifest.assets.map((a) => a.name) }, "no-store", 404);
  return new Response(null, {
    status: 302,
    headers: { location: `${r.origin}/${product}/${manifest.version}/${asset.name}`, "cache-control": SHORT, "x-release-version": manifest.version },
  });
}

async function api(r: Routed, segs: string[]): Promise<Response> {
  const origin = r.origin;
  if (segs.length === 0) return json(await readIndex(r));
  // Legacy aphrody API: /api/channels and /api/releases/<channel>/latest.
  if (segs.length === 2 && segs[1] === "latest" && isChannel(segs[0]!)) {
    const legacy = await r.env.RELEASES.get(`channels/${segs[0]}/latest.json`);
    if (legacy) return new Response(legacy.body, { headers: { "content-type": "application/json; charset=utf-8", "cache-control": SHORT } });
    const index = await readIndex(r);
    const hit = Object.values(index.products).map((p) => p[segs[0] as never] as ChannelPointer | undefined).find(Boolean);
    return json(hit ?? { ok: true, channel: segs[0], version: null, status: "pending_release" });
  }
  if (segs.length === 3 && segs[2] === "latest") {
    const manifest = await latestManifest(r, segs[0]!, segs[1]!);
    return manifest ? json(withUrls(manifest, origin)) : json({ error: "no release", product: segs[0], channel: segs[1] }, "no-store", 404);
  }
  if (segs.length === 2 && isSegment(segs[0]!) && isSegment(segs[1]!)) {
    const manifest = await readJson<ReleaseManifest>(r, releaseKeys.release(segs[0]!, segs[1]!));
    return manifest ? json(withUrls(manifest, origin), IMMUTABLE) : json({ error: "unknown release" }, "no-store", 404);
  }
  if (segs.length === 1 && segs[0] === "auto") {
    const obj = await r.env.RELEASES.get("auto/releases/manifest.json");
    return obj ? new Response(obj.body, { headers: { "content-type": "application/json; charset=utf-8", "cache-control": SHORT } }) : NOT_FOUND();
  }
  return NOT_FOUND();
}

const releases: Service = async (r) => {
  const { path, request } = r;
  if (request.method !== "GET" && request.method !== "HEAD") return new Response("method not allowed", { status: 405, headers: { allow: "GET, HEAD" } });
  if (path === "/health" || path === "/healthz") return json({ ok: true, service: "releases", channels: CHANNELS }, "no-store");
  if (path === "/" || path === "/download" || path.startsWith("/download/")) return portal(r);
  if (path === "/manifest.json") {
    if (!(await r.env.RELEASES.head(releaseKeys.index()))) return json(await readIndex(r), REVALIDATE);
    return serveObject(r.env.RELEASES, releaseKeys.index(), request, { cache: REVALIDATE, cors: true });
  }
  if (path === "/robots.txt") return new Response("User-agent: *\nAllow: /\n", { headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "public, max-age=3600" } });
  if (path === "/api/channels") return json({ channels: CHANNELS, latest: "/api/releases/{product}/{channel}/latest" });
  if (path.startsWith("/api/releases")) return api(r, path.slice("/api/releases".length).split("/").filter(Boolean));
  if (path === "/install.sh") return serveObject(r.env.RELEASES, "scripts/install.sh", request, { cache: "public, max-age=300" });
  if (path === "/install.ps1") return serveObject(r.env.RELEASES, "scripts/install.ps1", request, { cache: "public, max-age=300" });
  if (path === "/icon.svg" || path === "/favicon.ico" || path === "/apple-touch-icon.png") return serveObject(r.env.ASSETS, `brand${path}`, request, { cache: "public, max-age=3600", cors: true });
  if (path.startsWith("/brand/")) {
    const key = path.slice(1);
    return key.includes("..") ? NOT_FOUND() : serveObject(r.env.ASSETS, key, request, { cache: "public, max-age=3600", cors: true });
  }
  if (path.startsWith("/ay-tools/")) {
    const url = new URL(r.url);
    url.pathname = path.slice("/ay-tools".length);
    return toolsRelease.fetch(new Request(url, request), { RELEASES: r.env.CHROMIUM });
  }
  if (path.startsWith("/assets/")) {
    const key = decodeURIComponent(path.slice("/assets/".length));
    return key === "" || key.split("/").some((p) => p === ".." || p === "." || p === "") ? NOT_FOUND() : serveObject(r.env.RELEASES, key, request, { download: true });
  }
  const legacy = /^\/releases\/([^/]+)\/([^/]+)\/asset$/.exec(path);
  if (legacy && isChannel(legacy[1]!.toLowerCase())) {
    const os = r.url.searchParams.get("os") ?? "windows";
    const arch = r.url.searchParams.get("arch") ?? "x86_64";
    return serveObject(r.env.RELEASES, `manifests/${legacy[1]!.toLowerCase()}/${legacy[2]}/${os}-${arch}.json`, request, { cache: SHORT });
  }

  const segs = path.split("/").filter(Boolean).map(decodeURIComponent);
  const [product, second, third] = segs;
  if (!product || !isSegment(product)) return NOT_FOUND();
  if (second === "latest" && segs.length === 2) {
    const channel = (r.url.searchParams.get("channel") ?? "stable").toLowerCase();
    const manifest = await latestManifest(r, product, channel);
    return manifest ? json(withUrls(manifest, r.origin)) : json({ error: "no release", product, channel }, "no-store", 404);
  }
  if (second === "latest" && third && segs.length === 3) return latestRedirect(r, product, third);
  if (second && third && segs.length === 3 && isSegment(second) && isSegment(third)) {
    const key = releaseKeys.asset(product, second, third);
    if (second === "channels") return serveObject(r.env.RELEASES, key, request, { cache: REVALIDATE, cors: true });
    return serveObject(r.env.RELEASES, key, request, { download: third !== "SHA256SUMS" && !third.endsWith(".json"), cors: true });
  }
  return NOT_FOUND();
};

export default releases;
