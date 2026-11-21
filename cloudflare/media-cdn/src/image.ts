// Images: a prebuilt rung when the ladder has it, else one transform of the stored source, kept in R2 under its canonical hash.
import { IMAGE_MIME, type ImageFormat, type ImageRequest, formatImage, imageSourceKey, imageVariantKey, negotiate, tokensOf, variantOf } from "../../../tools/media-cdn/grammar.ts";
import type { ImageEntry } from "../../../tools/media-cdn/manifest.ts";
import { IMMUTABLE, MUTABLE, cached, problem, reply, serveObject, sha256Hex } from "./http.ts";

/** Bump when the bytes of an on-demand result change for the same inputs. */
const ALGORITHM = "1";
const DEFAULT_QUALITY: Record<ImageFormat, number | undefined> = { avif: 60, webp: 80, jpg: 82, png: undefined };

export interface Resolved {
  format: ImageFormat;
  /** Request after clamping to the source size: what the bytes really are. */
  effective: ImageRequest;
}

/** Never upscale: sizes above the source become the source size. */
export function resolve(entry: ImageEntry, sha: string, request: ImageRequest, accept: string | null): Resolved {
  const format = request.format ?? negotiate(accept, entry.fallback);
  const effective: ImageRequest = { ...request, pin: sha, format };
  const long = Math.max(entry.width, entry.height);
  if (effective.width !== undefined) effective.width = Math.min(effective.width, entry.width);
  if (effective.height !== undefined) effective.height = Math.min(effective.height, entry.height);
  if (effective.square !== undefined) effective.square = Math.min(effective.square, long);
  if (effective.width === undefined && effective.height === undefined && effective.square === undefined) effective.width = entry.width;
  return { format, effective };
}

export async function serveImage(request: Request, env: Env, ctx: ExecutionContext, entry: ImageEntry, sha: string, wanted: ImageRequest, started: number): Promise<Response> {
  const accept = request.headers.get("Accept");
  const { format, effective } = resolve(entry, sha, wanted, accept);
  const headers: Record<string, string> = {
    "Cache-Control": wanted.pin ? IMMUTABLE : MUTABLE,
    "Content-Location": `/v1/img/${formatImage(effective)}`,
  };
  if (!wanted.format) headers.Vary = "Accept";

  const produce = async (): Promise<Response> => {
    const variant = variantOf(effective);
    const prebuilt = variant && (variant.startsWith("w") ? entry.widths.includes(Number(variant.slice(1))) : entry.squares.includes(Number(variant.slice(1))));
    if (prebuilt) {
      const key = imageVariantKey(wanted.name, sha, variant, format);
      const served = await serveObject(request, env.MEDIA, key, { ...headers, "X-Media-Source": "r2" });
      if (served) return timed(served, started);
    }

    // On demand: the result of this exact need is a content-addressed object, built once.
    const hash = (await sha256Hex(`${ALGORITHM}|${sha}|${format}|${tokensOf(effective)}`)).slice(0, 20);
    const generated = `v1/gen/${hash}.${format}`;
    const stored = await serveObject(request, env.MEDIA, generated, { ...headers, "X-Media-Source": "r2" });
    if (stored) return timed(stored, started);
    if (!env.IMAGES) return problem(501, "this size is not prebuilt and on-demand resizing is not available", { variant: variant ?? tokensOf(effective) });

    const source = await env.MEDIA.get(imageSourceKey(wanted.name, sha, entry.source));
    if (!source) return problem(404, "image source missing", wanted.name);
    const transform: ImageTransform = {};
    if (effective.square !== undefined) Object.assign(transform, { width: effective.square, height: effective.square, fit: "scale-down" });
    else {
      if (effective.width !== undefined) transform.width = effective.width;
      if (effective.height !== undefined) transform.height = effective.height;
      transform.fit = effective.width !== undefined && effective.height !== undefined ? "cover" : "scale-down";
    }
    const quality = effective.quality ?? DEFAULT_QUALITY[format];
    let bytes: ArrayBuffer;
    try {
      const result = await env.IMAGES.input(source.body).transform(transform).output({ format: IMAGE_MIME[format] as ImageOutputOptions["format"], ...(quality !== undefined ? { quality } : {}) });
      bytes = await result.response().arrayBuffer();
    } catch (error) {
      return problem(502, "image transform failed", String(error));
    }
    ctx.waitUntil(env.MEDIA.put(generated, bytes, { httpMetadata: { contentType: IMAGE_MIME[format] } }));
    return timed(reply(200, bytes, { ...headers, "Content-Type": IMAGE_MIME[format], "Content-Length": String(bytes.byteLength), ETag: `"${hash}"`, "Accept-Ranges": "bytes", "X-Media-Source": "built" }), started);
  };
  return (await cached(request, ctx, `${format}-${tokensOf(effective)}`, produce)) as Response;
}

function timed(response: Response, started: number): Response {
  response.headers.set("Server-Timing", `gen;dur=${(performance.now() - started).toFixed(1)}`);
  return response;
}
