// R2 buckets served on their own hosts (the hosts that used to be R2 custom domains). A custom domain of R2 answers over HTTP/2 only; the same
// host routed to this Worker gets HTTP/3. Per host (vars.EDGE_BUCKETS, rendered from edge.toml [services.bucket.hosts]): the store (a name of
// edge.toml [stores], read as env.<store>), the CORS rule (vars.EDGE_CORS, the rule the bucket carried) and a fallback Cache-Control. Behaviour is that of the custom domain: GET and HEAD, ranges,
// ETag and Last-Modified validators, "/" and directory paths answer their index.html, no listing, Vary: Origin.
import { NOT_FOUND, serveObject } from "../r2.ts";
import type { EdgeEnv, Routed, Service } from "../types.ts";

export interface BucketHost {
  bucket: string;
  cors?: string;
  cache?: string;
}

export interface CorsRule {
  origins: string[];
  methods: string[];
  headers: string[];
  expose: string[];
  max_age: number;
}

const buckets = (env: EdgeEnv) => (env.EDGE_BUCKETS ?? {}) as Record<string, BucketHost>;
const rules = (env: EdgeEnv) => (env.EDGE_CORS ?? {}) as Record<string, CorsRule>;

/** CORS headers for a request Origin against a rule; empty when the origin is not allowed (the answer is then plain, as R2's is). */
export function corsHeaders(rule: CorsRule | undefined, origin: string | null): Headers {
  const h = new Headers();
  if (!rule) return h;
  h.append("vary", "Origin");
  if (origin && rule.origins.includes(origin)) {
    h.set("access-control-allow-origin", origin);
    if (rule.expose.length) h.set("access-control-expose-headers", rule.expose.join(","));
  }
  return h;
}

export function preflight(rule: CorsRule | undefined, request: Request): Response {
  const origin = request.headers.get("origin");
  const method = request.headers.get("access-control-request-method");
  const asked = (request.headers.get("access-control-request-headers") ?? "").split(",").map((s) => s.trim().toLowerCase()).filter(Boolean);
  const allowed = new Set(rule?.headers.map((s) => s.toLowerCase()));
  if (!rule || !origin || !rule.origins.includes(origin) || !method || !rule.methods.includes(method) || asked.some((a) => !allowed.has(a))) {
    return new Response(null, { status: 403, headers: { vary: "Origin, Access-Control-Request-Headers, Access-Control-Request-Method" } });
  }
  return new Response(null, {
    status: 200,
    headers: {
      "access-control-allow-origin": origin,
      "access-control-allow-methods": rule.methods.join(","),
      ...(asked.length ? { "access-control-allow-headers": asked.join(",") } : {}),
      "access-control-max-age": String(rule.max_age),
      vary: "Origin, Access-Control-Request-Headers, Access-Control-Request-Method",
    },
  });
}

/** Object key of a path: "/" and a trailing slash name the index.html below; traversal and control characters are refused. */
export function keyOfPath(pathname: string): string | undefined {
  let key: string;
  try {
    key = decodeURIComponent(pathname).replace(/^\/+/, "");
  } catch {
    return undefined;
  }
  if (key.includes("\0") || key.includes("\\") || key.split("/").some((p) => p === ".." || p === ".")) return undefined;
  return key === "" || key.endsWith("/") ? `${key}index.html` : key;
}

const bucket: Service = async (r: Routed) => {
  const host = buckets(r.env)[r.url.hostname.toLowerCase()];
  const binding = host && (r.env[host.bucket] as R2Bucket | undefined);
  if (!host || !binding) return NOT_FOUND();
  const rule = host.cors ? rules(r.env)[host.cors] : undefined;
  if (r.request.method === "OPTIONS") return preflight(rule, r.request);
  const key = keyOfPath(r.path);
  if (!key) return NOT_FOUND();
  return serveObject(binding, key, r.request, {
    cache: host.cache ?? null,
    preferMetadata: true,
    extra: corsHeaders(rule, r.request.headers.get("origin")),
  });
};

export default bucket;
