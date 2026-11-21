#!/usr/bin/env bun
// media-cdn step 3: check the live host by bytes and headers against its own manifest.
//   bun tools/media-cdn/verify.ts [--host https://media.aphrody.com] [--images N] [--videos N]
// Every rung of the sampled images and videos is fetched or range-read: status, Content-Type, Content-Length against the manifest, the file's
// magic bytes, cache headers, negotiation, conditional and range requests, an on-demand size, the sprite's symbol count and Brotli.
import { IMAGE_MIME, type ImageFormat, VIDEO_EXTENSION, VIDEO_MIME, imageVariantKey, spriteKey, videoFile, videoKey } from "./grammar.ts";
import type { Manifest } from "./manifest.ts";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1]! : fallback;
};
const host = opt("host", "https://media.aphrody.com").replace(/\/$/, "");
const imageLimit = Number(opt("images", "100"));
const videoLimit = Number(opt("videos", "100"));

let passed = 0;
const failures: string[] = [];
function check(name: string, ok: boolean, detail = "") {
  if (ok) passed++;
  else {
    failures.push(`${name} ${detail}`);
    console.log(`FAIL ${name} ${detail}`);
  }
}

const hex = (bytes: Uint8Array) => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
const ascii = (bytes: Uint8Array) => String.fromCharCode(...bytes);

/** What the first bytes of a file must be for its format. */
function magic(kind: string, head: Uint8Array): boolean {
  if (kind === "avif") return ascii(head.slice(4, 12)) === "ftypavif" || ascii(head.slice(4, 12)) === "ftypavis";
  if (kind === "webp") return ascii(head.slice(0, 4)) === "RIFF" && ascii(head.slice(8, 12)) === "WEBP";
  if (kind === "jpg") return hex(head.slice(0, 3)) === "ffd8ff";
  if (kind === "png") return hex(head.slice(0, 8)) === "89504e470d0a1a0a";
  if (kind === "mp4") return ascii(head.slice(4, 8)) === "ftyp";
  if (kind === "webm") return hex(head.slice(0, 4)) === "1a45dfa3";
  if (kind === "svg") return ascii(head).includes("<svg");
  return false;
}

async function get(path: string, init?: RequestInit) {
  return fetch(`${host}${path}`, { ...init, redirect: "manual" });
}

const indexResponse = await get("/v1/");
const index = (await indexResponse.json()) as { revision: string; images: number; video: number; sprites: number };
const manifestResponse = await get("/v1/manifest.json");
const manifest = (await manifestResponse.json()) as Manifest;
check("index revision equals manifest revision", index.revision === manifest.revision, `${index.revision} ${manifest.revision}`);
check("index counts equal manifest", index.images === Object.keys(manifest.images).length && index.video === Object.keys(manifest.video).length && index.sprites === Object.keys(manifest.sprites).length);
for (const [header, expected] of [["access-control-allow-origin", "*"], ["timing-allow-origin", "*"], ["cross-origin-resource-policy", "cross-origin"], ["x-content-type-options", "nosniff"]] as const)
  check(`index header ${header}`, indexResponse.headers.get(header) === expected, String(indexResponse.headers.get(header)));
check("HTTP/3 advertised", (indexResponse.headers.get("alt-svc") ?? "").includes("h3"), String(indexResponse.headers.get("alt-svc")));

// -------------------------------------------------------------------------------------------------------------------- images
let imageBytes = 0;
let imageFiles = 0;
for (const [name, entry] of Object.entries(manifest.images).slice(0, imageLimit)) {
  const formats: ImageFormat[] = ["avif", "webp", entry.fallback];
  const variants = [...entry.widths.map((w) => `w${w}`), ...entry.squares.map((s) => `s${s}`)];
  for (const variant of variants)
    for (const format of formats) {
      const url = `/v1/img/${name}.${entry.sha}.${format}=${variant}`;
      const response = await get(url);
      const body = new Uint8Array(await response.arrayBuffer());
      const expected = entry.bytes[`${variant}.${format}`];
      check(`${url} status`, response.status === 200, String(response.status));
      check(`${url} type`, response.headers.get("content-type") === IMAGE_MIME[format], String(response.headers.get("content-type")));
      check(`${url} bytes equal the manifest`, body.byteLength === expected, `${body.byteLength} vs ${expected}`);
      check(`${url} magic`, magic(format, body.slice(0, 16)));
      check(`${url} immutable`, (response.headers.get("cache-control") ?? "").includes("immutable"));
      imageBytes += body.byteLength;
      imageFiles++;
    }

  const widest = `w${entry.widths.at(-1)}`;
  const accepts: [string, ImageFormat][] = [["image/avif,image/webp,image/*,*/*;q=0.8", "avif"], ["image/webp,image/*,*/*;q=0.8", "webp"], ["*/*", entry.fallback]];
  for (const [accept, format] of accepts) {
    const response = await get(`/v1/img/${name}=${widest}`, { headers: { Accept: accept } });
    const body = new Uint8Array(await response.arrayBuffer());
    check(`${name} negotiates ${format}`, response.headers.get("content-type") === IMAGE_MIME[format] && magic(format, body.slice(0, 16)), String(response.headers.get("content-type")));
    check(`${name} Vary: Accept`, (response.headers.get("vary") ?? "").toLowerCase().includes("accept"));
  }
  const above = await get(`/v1/img/${name}.avif=w${Math.min(4096, entry.width + 500)}`);
  await above.arrayBuffer();
  check(`${name} never upscales`, (above.headers.get("content-location") ?? "").endsWith(`=w${Math.min(Math.min(4096, entry.width + 500), entry.width)}`), String(above.headers.get("content-location")));

  const first = await get(`/v1/img/${name}.avif=${widest}`);
  const etag = first.headers.get("etag");
  await first.arrayBuffer();
  const revalidated = await get(`/v1/img/${name}.avif=${widest}`, { headers: { "If-None-Match": etag ?? "" } });
  check(`${name} 304 on If-None-Match`, revalidated.status === 304, String(revalidated.status));

  // An off-ladder size: built once from the stored source, then read from the bucket.
  const odd = Math.max(16, Math.min(entry.width - 7, 777));
  const built = await get(`/v1/img/${name}.${entry.sha}.webp=w${odd}`);
  const builtBytes = new Uint8Array(await built.arrayBuffer());
  check(`${name} on demand w${odd}`, built.status === 200 && magic("webp", builtBytes.slice(0, 16)), `${built.status} ${built.headers.get("x-media-source")}`);
  await Bun.sleep(1500); // the result is written after the first answer has left
  const again = await get(`/v1/img/${name}.${entry.sha}.webp=w${odd}`);
  const againBytes = new Uint8Array(await again.arrayBuffer());
  check(`${name} on demand is stored`, again.headers.get("x-media-source") === "r2" && againBytes.byteLength === builtBytes.byteLength, String(again.headers.get("x-media-source")));
}

// -------------------------------------------------------------------------------------------------------------------- video
let videoRungs = 0;
let videoBytes = 0;
for (const [name, entry] of Object.entries(manifest.video).slice(0, videoLimit)) {
  for (const file of entry.files) {
    const path = `/${videoKey(name, entry.sha, videoFile(file.width, file.density, file.codec))}`;
    const head = await get(path, { method: "HEAD" });
    check(`${path} length equals the manifest`, Number(head.headers.get("content-length")) === file.bytes, `${head.headers.get("content-length")} vs ${file.bytes}`);
    check(`${path} type`, head.headers.get("content-type") === VIDEO_MIME[VIDEO_EXTENSION[file.codec]]);
    check(`${path} immutable and ranged`, (head.headers.get("cache-control") ?? "").includes("immutable") && head.headers.get("accept-ranges") === "bytes");
    const slice = await get(path, { headers: { Range: "bytes=0-31" } });
    const bytes = new Uint8Array(await slice.arrayBuffer());
    check(`${path} range 206`, slice.status === 206 && bytes.byteLength === 32 && slice.headers.get("content-range") === `bytes 0-31/${file.bytes}`, `${slice.status} ${slice.headers.get("content-range")}`);
    check(`${path} magic`, magic(VIDEO_EXTENSION[file.codec], bytes));
    if (file.codec === "av1") check(`${path} AV1 brand`, ascii(bytes).includes("av01"));
    videoRungs++;
    videoBytes += file.bytes;
  }
  const sources = (await (await get(`/v1/video/${name}.json`)).json()) as { sources: { src: string; type: string; media?: string }[] };
  check(`${name} sources`, sources.sources.length > 0 && sources.sources.every((s) => /codecs="[^"]+"/.test(s.type)));
  check(`${name} first source is AV1`, sources.sources[0]?.type.includes("av01") === true, String(sources.sources[0]?.type));
}

// -------------------------------------------------------------------------------------------------------------------- sprites
for (const [name, entry] of Object.entries(manifest.sprites)) {
  const response = await get(`/${spriteKey(name, entry.sha)}`, { headers: { "Accept-Encoding": "br, gzip" } });
  const text = await response.text();
  check(`sprite ${name} svg`, response.headers.get("content-type") === "image/svg+xml" && magic("svg", new TextEncoder().encode(text.slice(0, 200))));
  check(`sprite ${name} symbols`, (text.match(/<symbol /g) ?? []).length === entry.symbols.length, `${(text.match(/<symbol /g) ?? []).length} vs ${entry.symbols.length}`);
  check(`sprite ${name} immutable`, (response.headers.get("cache-control") ?? "").includes("immutable"));
  check(`sprite ${name} compressed`, ["br", "zstd", "gzip"].includes(response.headers.get("content-encoding") ?? ""), String(response.headers.get("content-encoding")));
}

// -------------------------------------------------------------------------------------------------------------------- errors
check("unknown image is 404", (await get("/v1/img/does-not-exist.avif")).status === 404);
check("bad token is 400", (await get("/v1/img/x.avif=w10")).status === 400);
check("POST is 405", (await get("/v1/", { method: "POST" })).status === 405);

console.log(`${passed} checks passed, ${failures.length} failed; ${imageFiles} image rungs (${imageBytes} B), ${videoRungs} video rungs (${videoBytes} B) on ${host}, manifest ${manifest.revision}`);
process.exit(failures.length ? 1 : 0);
