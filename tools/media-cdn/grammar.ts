// URL grammar of media.aphrody.com, shared by the Worker (cloudflare/media-cdn), the builder and the verifier, so the three never disagree.
//
//   /v1/img/<name>[.<sha8>][.<fmt>][=<tokens>]      image, tokens joined by "-": w600 h400 s144 q75
//   /v1/video/<name>[.<sha8>]/<W>w-<D>x-<codec>.<ext>   one rung of the ladder (W css px, D pixel density, codec av1|vp9|h264)
//   /v1/video/<name>[.<sha8>].json                  the <source> list of a video, in selection order
//   /v1/sprite/<name>[.<sha8>].svg                  one SVG sprite of <symbol>s
//   /v1/manifest.json                               every name, hash, size and rung
//
// `<sha8>` pins the first 8 hex digits of the content hash: pinned URLs are immutable, unpinned ones follow the manifest.
// Without `<fmt>` an image is negotiated from the Accept header (AVIF, WebP, then the fallback JPEG or PNG).

export const IMAGE_FORMATS = ["avif", "webp", "jpg", "png"] as const;
export type ImageFormat = (typeof IMAGE_FORMATS)[number];

export const IMAGE_MIME: Record<ImageFormat, string> = { avif: "image/avif", webp: "image/webp", jpg: "image/jpeg", png: "image/png" };

/** Widths and long-edge sizes built ahead of time for every image (never above the source). Other values are made on demand. */
export const WIDTH_LADDER = [320, 600, 1024, 1440, 1920, 2880, 3840] as const;
export const SQUARE_LADDER = [48, 72, 96, 144, 192, 512] as const;

export const CODECS = ["av1", "vp9", "h264"] as const;
export type Codec = (typeof CODECS)[number];
export const VIDEO_WIDTHS = [600, 1024, 1440, 1920] as const;
export const VIDEO_DENSITIES = [1, 2] as const;
/** Container per codec: the pairing every browser accepts. */
export const VIDEO_EXTENSION: Record<Codec, "mp4" | "webm"> = { av1: "mp4", vp9: "webm", h264: "mp4" };
export const VIDEO_MIME = { mp4: "video/mp4", webm: "video/webm" } as const;

/** Breakpoints measured on googlebook.google (min-width 600, 1024, 1440): the rung used below each one. */
export const VIDEO_BREAKPOINTS = [
  { media: "(max-width: 599px)", width: 600 },
  { media: "(min-width: 600px) and (max-width: 1023px)", width: 1024 },
  { media: "(min-width: 1440px)", width: 1920 },
  { media: "", width: 1440 },
] as const;
/** The codec order and density of one <source> row: the efficient codecs at 2x, the compatible one at 1x (as on googlebook.google). */
export const SOURCE_ORDER = [
  { codec: "av1", density: 2 },
  { codec: "vp9", density: 2 },
  { codec: "h264", density: 1 },
] as const;

export const SIZE_MIN = 16;
export const SIZE_MAX = 4096;
export const QUALITY_MIN = 30;
export const QUALITY_MAX = 95;

/**
 * Revision of the encoder settings, mixed into the content hash of a source. A change of settings (quality, subsampling, codec flags)
 * bumps its revision, so the new bytes get new immutable URLs and the pinned URLs of the old ones keep their old bytes.
 */
export const PIPELINE = { image: "2", video: "1" } as const;

export class GrammarError extends Error {}

export interface ImageRequest {
  name: string;
  pin?: string;
  format?: ImageFormat;
  width?: number;
  height?: number;
  /** Long edge of the result: the image fits a square of this size, aspect kept (Google's `=s144`). */
  square?: number;
  quality?: number;
}

const NAME = /^[a-z0-9][a-z0-9_-]*(\/[a-z0-9][a-z0-9_-]*)*$/;
const PIN = /^[0-9a-f]{8}$/;

export function validName(name: string): boolean {
  return NAME.test(name) && name.length <= 96;
}

function size(token: string, value: string): number {
  if (!/^\d+$/.test(value)) throw new GrammarError(`${token}: an integer is required`);
  const n = Number(value);
  if (n < SIZE_MIN || n > SIZE_MAX) throw new GrammarError(`${token}: ${SIZE_MIN}..${SIZE_MAX}`);
  return n;
}

/** Parses what follows `/v1/img/`. */
export function parseImage(path: string): ImageRequest {
  const eq = path.indexOf("=");
  const head = eq < 0 ? path : path.slice(0, eq);
  const tokens = eq < 0 ? "" : path.slice(eq + 1);
  let rest = head;
  let format: ImageFormat | undefined;
  let pin: string | undefined;
  const ext = /\.([a-z0-9]+)$/.exec(rest)?.[1];
  if (ext && (IMAGE_FORMATS as readonly string[]).includes(ext)) {
    format = ext as ImageFormat;
    rest = rest.slice(0, -ext.length - 1);
  } else if (ext === "jpeg") {
    format = "jpg";
    rest = rest.slice(0, -5);
  }
  const hash = /\.([0-9a-f]{8})$/.exec(rest)?.[1];
  if (hash && PIN.test(hash)) {
    pin = hash;
    rest = rest.slice(0, -9);
  }
  if (!validName(rest)) throw new GrammarError(`invalid image name: ${rest}`);
  const request: ImageRequest = { name: rest };
  if (pin) request.pin = pin;
  if (format) request.format = format;
  if (tokens) {
    for (const token of tokens.split("-")) {
      const key = token[0];
      const value = token.slice(1);
      if (key === "w") request.width = size("w", value);
      else if (key === "h") request.height = size("h", value);
      else if (key === "s") request.square = size("s", value);
      else if (key === "q") {
        if (!/^\d+$/.test(value) || Number(value) < QUALITY_MIN || Number(value) > QUALITY_MAX) throw new GrammarError(`q: ${QUALITY_MIN}..${QUALITY_MAX}`);
        request.quality = Number(value);
      } else throw new GrammarError(`unknown token: ${token}`);
    }
    if (request.square !== undefined && (request.width !== undefined || request.height !== undefined)) throw new GrammarError("s cannot be combined with w or h");
  }
  return request;
}

/** Tokens in canonical order: the same need always has the same text. */
export function tokensOf(request: Pick<ImageRequest, "width" | "height" | "square" | "quality">): string {
  const out: string[] = [];
  if (request.width !== undefined) out.push(`w${request.width}`);
  if (request.height !== undefined) out.push(`h${request.height}`);
  if (request.square !== undefined) out.push(`s${request.square}`);
  if (request.quality !== undefined) out.push(`q${request.quality}`);
  return out.join("-");
}

/** The URL path (below `/v1/img/`) of a request, canonical. */
export function formatImage(request: ImageRequest): string {
  const tokens = tokensOf(request);
  return `${request.name}${request.pin ? `.${request.pin}` : ""}${request.format ? `.${request.format}` : ""}${tokens ? `=${tokens}` : ""}`;
}

/** R2 key of an image rung built ahead of time: `v1/img/<name>.<sha8>/w600.avif`. */
export const imageVariantKey = (name: string, sha: string, variant: string, format: ImageFormat) => `v1/img/${name}.${sha}/${variant}.${format}`;
export const imageSourceKey = (name: string, sha: string, ext: string) => `v1/src/img/${name}.${sha}.${ext}`;
export const videoKey = (name: string, sha: string, file: string) => `v1/video/${name}.${sha}/${file}`;
export const spriteKey = (name: string, sha: string) => `v1/sprite/${name}.${sha}.svg`;

export const videoFile = (width: number, density: number, codec: Codec) => `${width}w-${density}x-${codec}.${VIDEO_EXTENSION[codec]}`;

export interface VideoFileRequest {
  width: number;
  density: number;
  codec: Codec;
}

export function parseVideoFile(file: string): VideoFileRequest | undefined {
  const m = /^(\d+)w-(\d)x-(av1|vp9|h264)\.(mp4|webm)$/.exec(file);
  if (!m) return undefined;
  const codec = m[3] as Codec;
  if (VIDEO_EXTENSION[codec] !== m[4]) return undefined;
  return { width: Number(m[1]), density: Number(m[2]), codec };
}

/** The prebuilt variant label of an image request, when it is exactly one rung of a ladder: `w600` or `s144`. */
export function variantOf(request: ImageRequest): string | undefined {
  if (request.quality !== undefined || request.height !== undefined) return undefined;
  if (request.width !== undefined && request.square === undefined) return `w${request.width}`;
  if (request.square !== undefined && request.width === undefined) return `s${request.square}`;
  return undefined;
}

/** What the browser can decode, from an Accept header: the first of AVIF, WebP the client lists, else the fallback format. */
export function negotiate(accept: string | null, fallback: ImageFormat): ImageFormat {
  const list = (accept ?? "").toLowerCase();
  if (list.includes("image/avif")) return "avif";
  if (list.includes("image/webp")) return "webp";
  return fallback;
}
