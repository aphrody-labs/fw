// media.aphrody.com: the media CDN of the monorepo.
//
// Everything lives in the R2 bucket MEDIA under `v1/` (tools/media-cdn builds and publishes it, see tools/media-cdn/grammar.ts for the URLs):
//   img/<name>.<sha8>/w600.avif ...   images, AVIF and WebP and a JPEG or PNG fallback, at a ladder of widths and long-edge sizes
//   video/<name>.<sha8>/1920w-2x-av1.mp4 ...   the AV1, VP9 and H.264 ladder, at 600, 1024, 1440 and 1920 css px, 1x and 2x
//   sprite/<name>.<sha8>.svg          one SVG sprite
//   manifest.json                     names, hashes, sizes
// Sizes outside the ladder are made on demand by the Images binding from the stored source and kept in R2 under their canonical hash.
//
// Cache policy
//   `.<sha8>` pinned names: public, max-age=31536000, immutable (the hash is part of the key, so the bytes can never change)
//   names that follow the manifest: max-age=300, s-maxage=3600, stale-while-revalidate=86400, revalidated through the ETag
// Images without a format extension are negotiated from Accept and answer `Vary: Accept`.
import { GrammarError, VIDEO_BREAKPOINTS, parseImage, parseVideoFile, spriteKey, validName, videoKey } from "../../tools/media-cdn/grammar.ts";
import { sourcesOf } from "../../tools/media-cdn/manifest.ts";
import { IMMUTABLE, MUTABLE, cached, problem, reply, serveObject } from "./src/http.ts";
import { serveImage } from "./src/image.ts";
import { imageEntry, loadManifest, spriteSha, videoEntry } from "./src/store.ts";

const PIN = /^(.+)\.([0-9a-f]{8})$/;

function splitPin(stem: string): { name: string; pin?: string } {
  const m = PIN.exec(stem);
  return m ? { name: m[1]!, pin: m[2]! } : { name: stem };
}

const notFound = (what: string) => problem(404, "not found", what);

async function index(env: Env, url: URL): Promise<Response> {
  const manifest = await loadManifest(env.MEDIA);
  if (!manifest) return problem(503, "manifest not published yet");
  const body = {
    revision: manifest.revision,
    images: Object.keys(manifest.images).length,
    video: Object.keys(manifest.video).length,
    sprites: Object.keys(manifest.sprites).length,
    routes: {
      image: `${url.origin}/v1/img/<name>[.<sha8>][.avif|webp|jpg|png][=w<px>-h<px>|s<px>-q<30..95>]`,
      video: `${url.origin}/v1/video/<name>[.<sha8>]/<600|1024|1440|1920>w-<1|2>x-<av1|vp9|h264>.<mp4|webm>`,
      sources: `${url.origin}/v1/video/<name>[.<sha8>].json`,
      sprite: `${url.origin}/v1/sprite/<name>[.<sha8>].svg`,
      manifest: `${url.origin}/v1/manifest.json`,
      fonts: "https://material.aphrody.com/v1/",
    },
  };
  return reply(200, JSON.stringify(body, null, 2) + "\n", { "Content-Type": "application/json; charset=utf-8", "Cache-Control": MUTABLE });
}

async function route(request: Request, env: Env, ctx: ExecutionContext, url: URL): Promise<Response> {
  const started = performance.now();
  const path = url.pathname;
  if (path === "/v1" || path === "/v1/") return index(env, url);

  if (path === "/v1/manifest.json") {
    return (await serveObject(request, env.MEDIA, "v1/manifest.json", { "Cache-Control": "public, max-age=60, s-maxage=60, stale-while-revalidate=600" })) ?? problem(503, "manifest not published yet");
  }

  if (path.startsWith("/v1/img/")) {
    const request_ = parseImage(path.slice("/v1/img/".length));
    const found = await imageEntry(env.MEDIA, request_.name, request_.pin);
    if (!found) return notFound(request_.name);
    return serveImage(request, env, ctx, found.entry, found.sha, request_, started);
  }

  if (path.startsWith("/v1/video/")) {
    const rest = path.slice("/v1/video/".length);
    if (rest.endsWith(".json")) {
      const { name, pin } = splitPin(rest.slice(0, -5));
      if (!validName(name)) throw new GrammarError(`invalid video name: ${name}`);
      const found = await videoEntry(env.MEDIA, name, pin);
      if (!found) return notFound(name);
      const body = { name, sha: found.sha, duration: found.entry.duration, fps: found.entry.fps, poster: found.entry.poster, breakpoints: VIDEO_BREAKPOINTS, sources: sourcesOf(name, found.entry, url.origin) };
      return reply(200, JSON.stringify(body, null, 2) + "\n", { "Content-Type": "application/json; charset=utf-8", "Cache-Control": pin ? IMMUTABLE : MUTABLE });
    }
    const slash = rest.lastIndexOf("/");
    if (slash < 0) return notFound(rest);
    const { name, pin } = splitPin(rest.slice(0, slash));
    const file = rest.slice(slash + 1);
    if (!validName(name)) throw new GrammarError(`invalid video name: ${name}`);
    if (!parseVideoFile(file)) throw new GrammarError(`invalid rung: ${file}`);
    const found = await videoEntry(env.MEDIA, name, pin);
    if (!found) return notFound(name);
    const served = await serveObject(request, env.MEDIA, videoKey(name, found.sha, file), { "Cache-Control": pin ? IMMUTABLE : MUTABLE, "X-Media-Source": "r2" });
    return served ?? notFound(`${name}/${file}`);
  }

  if (path.startsWith("/v1/sprite/") && path.endsWith(".svg")) {
    const { name, pin } = splitPin(path.slice("/v1/sprite/".length, -4));
    if (!validName(name)) throw new GrammarError(`invalid sprite name: ${name}`);
    const sha = await spriteSha(env.MEDIA, name, pin);
    if (!sha) return notFound(name);
    const served = await cached(request, ctx, sha, () => serveObject(request, env.MEDIA, spriteKey(name, sha), { "Cache-Control": pin ? IMMUTABLE : MUTABLE, "X-Media-Source": "r2" }));
    return served ?? notFound(name);
  }

  return notFound(path);
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    if (request.method === "OPTIONS") return reply(204, null, { "Access-Control-Max-Age": "86400" });
    if (request.method !== "GET" && request.method !== "HEAD") return problem(405, "method not allowed");
    try {
      return await route(request, env, ctx, new URL(request.url));
    } catch (error) {
      if (error instanceof GrammarError) return problem(400, error.message);
      throw error;
    }
  },
} satisfies ExportedHandler<Env>;
