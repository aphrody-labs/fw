import { Hono } from "hono";
import { z } from "zod";
import skillsIndex from "./agent-skills-index.json";

export interface GatewayEnv {
  ASSETS: R2Bucket;
  VIDEO: R2Bucket;
}

const IMMUTABLE_CACHE = "public, max-age=31536000, immutable";
const VIDEO_KINDS = new Set(["loop", "stream", "segment"]);
const AssetPathSchema = z.object({
  kind: z.enum(["icon", "ui", "hdri", "lottie", "tex", "glb", "loop", "stream", "segment", "index"]),
  key: z.string().regex(/^[0-9a-f]{64}\.[a-z0-9]+$/i).or(z.literal("latest.json")),
});

function parseSingleRange(rangeHeader: string | undefined): R2Range | undefined {
  if (!rangeHeader) return undefined;
  const m = /^bytes=(\d+)-(\d*)$/.exec(rangeHeader.trim());
  if (!m) return undefined;
  const offset = Number(m[1]);
  if (!Number.isSafeInteger(offset) || offset < 0) return undefined;
  if (!m[2]) return { offset };
  const end = Number(m[2]);
  if (!Number.isSafeInteger(end) || end < offset) return undefined;
  return { offset, length: end - offset + 1 };
}

function applyCorsAndCacheHeaders(headers: Headers, isIndex: boolean): void {
  headers.set("Accept-Ranges", "bytes");
  headers.set(
    "Cache-Control",
    isIndex ? "public, max-age=60, must-revalidate" : IMMUTABLE_CACHE,
  );
  headers.set("Access-Control-Allow-Origin", "*");
  headers.set("Access-Control-Expose-Headers", "ETag, Content-Length, Content-Range, Accept-Ranges");
}

export const app = new Hono<{ Bindings: GatewayEnv }>();

app.get("/healthz", (c) => {
  return c.json({ ok: true, service: "ay-cf-gateway", runtime: "cloudflare-workers" });
});

app.get("/.well-known/agent-skills/index.json", (c) => {
  c.header("Cache-Control", "public, max-age=300");
  return c.json(skillsIndex);
});

app.on(["GET", "HEAD"], "/v1/:kind/:key", async (c) => {
  const parsed = AssetPathSchema.safeParse({
    kind: c.req.param("kind"),
    key: c.req.param("key"),
  });
  if (!parsed.success) {
    return c.json({ error: "invalid asset path: expected /v1/<kind>/<sha256>.<ext> or /v1/<kind>/latest.json" }, 400);
  }

  const { kind, key } = parsed.data;
  const objectKey = `v1/${kind}/${key}`;
  const isIndex = kind === "index";
  const bucket = VIDEO_KINDS.has(kind) ? c.env.VIDEO : c.env.ASSETS;

  const ifNoneMatch = c.req.header("if-none-match");
  if (c.req.method === "HEAD") {
    const headObj = await bucket.head(objectKey);
    if (!headObj) return c.notFound();
    const headers = new Headers();
    headObj.writeHttpMetadata(headers);
    headers.set("ETag", headObj.httpEtag);
    headers.set("Content-Length", String(headObj.size));
    applyCorsAndCacheHeaders(headers, isIndex);
    return new Response(null, { status: 200, headers });
  }

  const range = parseSingleRange(c.req.header("range"));
  const obj = await bucket.get(objectKey, {
    ...(range ? { range } : {}),
    ...(ifNoneMatch ? { onlyIf: { etagDoesNotMatch: ifNoneMatch } } : {}),
  });

  if (!obj) return c.notFound();
  if (!("body" in obj)) {
    const headers = new Headers();
    applyCorsAndCacheHeaders(headers, isIndex);
    return new Response(null, { status: 304, headers });
  }

  const headers = new Headers();
  obj.writeHttpMetadata(headers);
  headers.set("ETag", obj.httpEtag);
  applyCorsAndCacheHeaders(headers, isIndex);

  if (obj.range && "offset" in obj.range && "length" in obj.range) {
    const start = obj.range.offset ?? 0;
    const len = obj.range.length ?? obj.size;
    const end = start + len - 1;
    headers.set("Content-Range", `bytes ${start}-${end}/${obj.size}`);
    headers.set("Content-Length", String(len));
    return new Response(obj.body, { status: 206, headers });
  }

  headers.set("Content-Length", String(obj.size));
  return new Response(obj.body, { status: 200, headers });
});

export default app;
