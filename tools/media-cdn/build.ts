#!/usr/bin/env bun
// media-cdn step 1: turn a directory of sources into the /v1 tree served by cloudflare/media-cdn.
//   bun tools/media-cdn/build.ts --src DIR --out DIR [--jobs N] [--only img,video,sprite]
//
// Source layout (names are the lower-case relative paths without extension, see grammar.ts validName):
//   DIR/img/<name>.(png|jpg|jpeg|webp|avif)    -> w<ladder> and s<ladder> rungs in AVIF, WebP and a JPEG (opaque) or PNG (alpha) fallback
//   DIR/video/<name>.(mp4|mov|mkv|webm)        -> <600|1024|1440|1920>w-<1|2>x-<av1|vp9|h264> rungs plus a poster image <name>-poster
//   DIR/sprite/<name>/<symbol>.svg             -> one sprite with a <symbol id="<symbol>"> per file
// Encoders run where this script runs: ffmpeg (libsvtav1, libvpx-vp9, libx264), ffprobe, vips, avifenc, cwebp and magick on the cluster
// nodes (fr1, prd), never on the PC. Existing outputs are kept, so an interrupted run resumes.
import { mkdir, readdir, rename, rm, stat } from "node:fs/promises";
import { basename, dirname, extname, join, relative, resolve, sep } from "node:path";
import { cpus, tmpdir } from "node:os";
import {
  CODECS,
  SQUARE_LADDER,
  VIDEO_DENSITIES,
  VIDEO_EXTENSION,
  VIDEO_WIDTHS,
  WIDTH_LADDER,
  type Codec,
  type ImageFormat,
  imageSourceKey,
  imageVariantKey,
  spriteKey,
  validName,
  videoFile,
  videoKey,
} from "./grammar.ts";
import { symbolOf } from "./sprite.ts";
import type { ImageEntry, Manifest, SpriteEntry, VideoEntry, VideoFile } from "./manifest.ts";

const args = process.argv.slice(2);
const opt = (name: string, fallback?: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1]! : fallback;
};
const src = resolve(opt("src") ?? fail("--src DIR is required"));
const out = resolve(opt("out") ?? fail("--out DIR is required"));
const jobs = Number(opt("jobs", String(Math.max(1, Math.floor(cpus().length / 4)))));
const only = new Set((opt("only", "img,video,sprite") ?? "").split(","));

function fail(message: string): never {
  console.error(message);
  process.exit(2);
}

for (const tool of ["ffmpeg", "ffprobe", "vips", "avifenc", "cwebp", "magick"]) if (!Bun.which(tool)) fail(`missing tool on this host: ${tool}`);

async function run(cmd: string[]): Promise<string> {
  const proc = Bun.spawn(cmd, { stdout: "pipe", stderr: "pipe" });
  const [stdout, stderr, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  if (code !== 0) throw new Error(`${cmd.join(" ")}\n${stderr.trim().split("\n").slice(-6).join("\n")}`);
  return stdout;
}

async function pool<T>(items: T[], work: (item: T) => Promise<void>) {
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(jobs, items.length) }, async () => {
      while (next < items.length) await work(items[next++]!);
    }),
  );
}

async function walk(dir: string): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(dir, { withFileTypes: true }).catch(() => [])) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...(await walk(path)));
    else found.push(path);
  }
  return found.sort();
}

const nameOf = (root: string, file: string) => {
  const rel = relative(root, file).split(sep).join("/");
  const name = rel.slice(0, rel.length - extname(rel).length).toLowerCase();
  if (!validName(name)) throw new Error(`${file}: "${name}" is not a valid media name (a-z 0-9 _ - and / between folders)`);
  return name;
};

async function sha256(file: string): Promise<string> {
  const hasher = new Bun.CryptoHasher("sha256");
  for await (const chunk of Bun.file(file).stream()) hasher.update(chunk);
  return hasher.digest("hex");
}

const exists = async (path: string) => stat(path).then((s) => s.size > 0, () => false);

/** Writes through a temporary name so a killed encoder never leaves a truncated rung behind. */
async function produce(target: string, make: (temporary: string) => Promise<void>) {
  if (await exists(target)) return;
  await mkdir(dirname(target), { recursive: true });
  const temporary = join(dirname(target), `.${basename(target)}.partial${extname(target)}`);
  await make(temporary);
  await rename(temporary, target);
}

const tree = (key: string) => join(out, key);
const scratch = join(tmpdir(), `media-cdn-${process.pid}`);
await mkdir(scratch, { recursive: true });

interface Probe {
  width: number;
  height: number;
  pixFmt: string;
  fps: number;
  duration: number;
  level: number;
  profile: string;
  bitRate: number;
}

async function probe(file: string): Promise<Probe> {
  const json = JSON.parse(await run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,pix_fmt,r_frame_rate,level,profile,bit_rate:format=duration,bit_rate", "-of", "json", file]));
  const stream = json.streams?.[0];
  if (!stream) throw new Error(`${file}: no video stream`);
  const [num, den] = String(stream.r_frame_rate ?? "0/1").split("/").map(Number);
  return {
    width: stream.width,
    height: stream.height,
    pixFmt: stream.pix_fmt ?? "",
    fps: den ? num! / den : 0,
    duration: Number(json.format?.duration ?? 0),
    level: Number(stream.level ?? -99),
    profile: String(stream.profile ?? ""),
    bitRate: Number(stream.bit_rate ?? json.format?.bit_rate ?? 0),
  };
}

// ---------------------------------------------------------------------------------------------------------------- images

const QUALITY = { avif: 60, webp: 80, jpg: 82 };

async function encodeImage(intermediate: string, format: ImageFormat, target: string) {
  await produce(target, async (temporary) => {
    if (format === "avif") await run(["avifenc", "-q", String(QUALITY.avif), "--speed", "6", "-j", "2", intermediate, temporary]);
    else if (format === "webp") await run(["cwebp", "-quiet", "-q", String(QUALITY.webp), "-m", "5", "-alpha_q", "90", "-metadata", "none", intermediate, "-o", temporary]);
    else if (format === "jpg") await run(["magick", intermediate, "-background", "white", "-alpha", "remove", "-alpha", "off", "-quality", String(QUALITY.jpg), "-sampling-factor", "4:2:0", "-interlace", "Plane", "-strip", `jpg:${temporary}`]);
    else await run(["magick", intermediate, "-strip", "-define", "png:compression-level=9", `png:${temporary}`]);
  });
}

async function buildImage(name: string, file: string): Promise<ImageEntry> {
  const sha = (await sha256(file)).slice(0, 8);
  const meta = await probe(file);
  const alpha = /a(?:\d|p|$)|ya|pal8|rgba|bgra|argb/.test(meta.pixFmt) && meta.pixFmt !== "";
  const fallback: ImageEntry["fallback"] = alpha ? "png" : "jpg";
  const long = Math.max(meta.width, meta.height);
  const widths = [...WIDTH_LADDER.filter((w) => w < meta.width), meta.width];
  const squares = SQUARE_LADDER.filter((s) => s <= long);
  const ext = extname(file).slice(1).toLowerCase().replace("jpeg", "jpg");
  const sourceTarget = tree(imageSourceKey(name, sha, ext));
  await produce(sourceTarget, async (temporary) => {
    await Bun.write(temporary, Bun.file(file));
  });

  const rungs = [...widths.map((w) => ({ variant: `w${w}`, width: w, height: 10_000_000 })), ...squares.map((s) => ({ variant: `s${s}`, width: s, height: s }))];
  const bytes: Record<string, number> = {};
  for (const rung of rungs) {
    const intermediate = join(scratch, `${name.replaceAll("/", "_")}.${sha}.${rung.variant}.png`);
    // Rungs at the native size are not resampled; vips copies the pixels and normalises the colour profile to sRGB.
    await run(["vips", "thumbnail", file, `${intermediate}[compression=1]`, String(rung.width), "--height", String(rung.height), "--size", "down", "--export-profile", "srgb"]);
    for (const format of ["avif", "webp", fallback] as ImageFormat[]) {
      const target = tree(imageVariantKey(name, sha, rung.variant, format));
      await encodeImage(intermediate, format, target);
      bytes[`${rung.variant}.${format}`] = (await stat(target)).size;
    }
    await rm(intermediate, { force: true });
  }
  const entry: ImageEntry = { sha, source: ext, width: meta.width, height: meta.height, alpha, fallback, widths, squares, bytes };
  await Bun.write(tree(`v1/img/${name}.${sha}/entry.json`), JSON.stringify(entry));
  return entry;
}

// ---------------------------------------------------------------------------------------------------------------- video

const hex = (n: number) => n.toString(16).padStart(2, "0");

/** Level of the codec string from the encoded frame, the way a decoder's capability check reads it. */
function codecString(codec: Codec, p: Probe): string {
  const pixels = p.width * p.height;
  if (codec === "av1") {
    const idx = p.level >= 0 ? p.level : pixels <= 147_456 ? 0 : pixels <= 278_784 ? 1 : pixels <= 665_856 ? 4 : pixels <= 1_065_024 ? 5 : pixels <= 2_359_296 ? 8 : pixels <= 8_912_896 ? 12 : 16;
    return `av01.0.${String(idx).padStart(2, "0")}M.08`;
  }
  if (codec === "vp9") {
    const level = pixels <= 36_864 ? 10 : pixels <= 73_728 ? 11 : pixels <= 122_880 ? 20 : pixels <= 245_760 ? 21 : pixels <= 552_960 ? 30 : pixels <= 983_040 ? 31 : pixels <= 2_228_224 ? 40 : pixels <= 8_912_896 ? 50 : 60;
    return `vp09.00.${level}.08`;
  }
  const profile = p.profile.toLowerCase().startsWith("high") ? 100 : p.profile.toLowerCase().startsWith("main") ? 77 : 66;
  return `avc1.${hex(profile)}00${hex(p.level > 0 ? p.level : 40)}`;
}

function encoderArgs(codec: Codec): string[] {
  if (codec === "av1") return ["-c:v", "libsvtav1", "-preset", "8", "-crf", "34", "-g", "60", "-pix_fmt", "yuv420p", "-movflags", "+faststart"];
  if (codec === "vp9") return ["-c:v", "libvpx-vp9", "-crf", "36", "-b:v", "0", "-deadline", "good", "-cpu-used", "4", "-row-mt", "1", "-tile-columns", "2", "-g", "60", "-pix_fmt", "yuv420p"];
  return ["-c:v", "libx264", "-preset", "slow", "-crf", "24", "-profile:v", "high", "-pix_fmt", "yuv420p", "-g", "60", "-movflags", "+faststart"];
}

async function buildVideo(name: string, file: string): Promise<{ entry: VideoEntry; poster?: { name: string; file: string } }> {
  const sha = (await sha256(file)).slice(0, 8);
  const meta = await probe(file);
  const files: VideoFile[] = [];
  const rungs: { width: number; density: number; codec: Codec }[] = [];
  for (const width of VIDEO_WIDTHS)
    for (const density of VIDEO_DENSITIES)
      for (const codec of CODECS) {
        // A rung is never upscaled: 1920 css px at 2x needs a 3840 px source.
        if (width * density <= meta.width) rungs.push({ width, density, codec });
      }
  await pool(rungs, async ({ width, density, codec }) => {
    const target = tree(videoKey(name, sha, videoFile(width, density, codec)));
    await produce(target, (temporary) =>
      run(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i", file, "-an", "-vf", `scale=${width * density}:-2:flags=lanczos`, "-r", String(meta.fps || 30), ...encoderArgs(codec), "-f", VIDEO_EXTENSION[codec] === "webm" ? "webm" : "mp4", temporary]).then(() => undefined),
    );
  });
  for (const { width, density, codec } of rungs) {
    const target = tree(videoKey(name, sha, videoFile(width, density, codec)));
    const encoded = await probe(target);
    files.push({ width, density, codec, pixels: { width: encoded.width, height: encoded.height }, bytes: (await stat(target)).size, codecs: codecString(codec, encoded), bitrate: encoded.bitRate });
  }
  // The poster is a frame from the middle of the clip, published as an image named <name>-poster.
  const posterFile = join(scratch, `${name.replaceAll("/", "_")}-poster.png`);
  await run(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-ss", String(meta.duration / 2), "-i", file, "-frames:v", "1", posterFile]);
  const entry: VideoEntry = { sha, duration: Number(meta.duration.toFixed(3)), fps: Number(meta.fps.toFixed(3)), poster: `${name}-poster`, files };
  await Bun.write(tree(`v1/video/${name}.${sha}/entry.json`), JSON.stringify(entry));
  return { entry, poster: { name: `${name}-poster`, file: posterFile } };
}

// ---------------------------------------------------------------------------------------------------------------- sprites

async function buildSprite(name: string, dir: string): Promise<SpriteEntry> {
  const files = (await walk(dir)).filter((f) => f.endsWith(".svg"));
  const symbols: string[] = [];
  const body: string[] = [];
  for (const file of files) {
    const id = nameOf(dir, file).replaceAll("/", "-");
    symbols.push(id);
    body.push(symbolOf(id, await Bun.file(file).text()));
  }
  const text = `<svg xmlns="http://www.w3.org/2000/svg">${body.join("")}</svg>\n`;
  const sha = new Bun.CryptoHasher("sha256").update(text).digest("hex").slice(0, 8);
  await Bun.write(tree(spriteKey(name, sha)), text);
  return { sha, symbols, bytes: new TextEncoder().encode(text).byteLength };
}

// ---------------------------------------------------------------------------------------------------------------- main

const manifest: Manifest = { schema: 1, revision: "", images: {}, video: {}, sprites: {} };
const IMAGE_EXT = new Set([".png", ".jpg", ".jpeg", ".webp", ".avif"]);
const VIDEO_EXT = new Set([".mp4", ".mov", ".mkv", ".webm"]);
const started = performance.now();

const posters: { name: string; file: string }[] = [];
if (only.has("video")) {
  const root = join(src, "video");
  for (const file of (await walk(root)).filter((f) => VIDEO_EXT.has(extname(f).toLowerCase()))) {
    const name = nameOf(root, file);
    const built = await buildVideo(name, file);
    manifest.video[name] = built.entry;
    if (built.poster) posters.push(built.poster);
    console.log(`video ${name}.${built.entry.sha}: ${built.entry.files.length} rungs, ${built.entry.files.reduce((n, f) => n + f.bytes, 0)} bytes`);
  }
}
if (only.has("img")) {
  const root = join(src, "img");
  const images = (await walk(root)).filter((f) => IMAGE_EXT.has(extname(f).toLowerCase())).map((file) => ({ name: nameOf(root, file), file }));
  await pool([...images, ...posters], async ({ name, file }) => {
    manifest.images[name] = await buildImage(name, file);
    console.log(`img ${name}.${manifest.images[name]!.sha}: ${manifest.images[name]!.widths.length} widths, ${manifest.images[name]!.squares.length} squares`);
  });
} else
  await pool(posters, async ({ name, file }) => {
    manifest.images[name] = await buildImage(name, file);
  });
if (only.has("sprite")) {
  const root = join(src, "sprite");
  for (const entry of await readdir(root, { withFileTypes: true }).catch(() => [])) {
    if (!entry.isDirectory()) continue;
    manifest.sprites[entry.name] = await buildSprite(entry.name, join(root, entry.name));
    console.log(`sprite ${entry.name}.${manifest.sprites[entry.name]!.sha}: ${manifest.sprites[entry.name]!.symbols.length} symbols, ${manifest.sprites[entry.name]!.bytes} bytes`);
  }
}

const sorted = <T>(record: Record<string, T>) => Object.fromEntries(Object.entries(record).sort(([a], [b]) => a.localeCompare(b)));
manifest.images = sorted(manifest.images);
manifest.video = sorted(manifest.video);
manifest.sprites = sorted(manifest.sprites);
manifest.revision = new Bun.CryptoHasher("sha256").update(JSON.stringify({ ...manifest, revision: "" })).digest("hex").slice(0, 8);
await Bun.write(tree("v1/manifest.json"), JSON.stringify(manifest, null, 1) + "\n");
await rm(scratch, { recursive: true, force: true });
console.log(`manifest ${manifest.revision}: ${Object.keys(manifest.images).length} images, ${Object.keys(manifest.video).length} videos, ${Object.keys(manifest.sprites).length} sprites in ${((performance.now() - started) / 1000).toFixed(1)} s -> ${out}`);
