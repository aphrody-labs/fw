// Drive CDN mounted on fw-edge at /drive/*: Cloudflare Access (Google IdP for people, service tokens for
// agents and apps) -> R2 cache keyed by file version -> Google Drive API read through a service account
// that the folder is shared with. One path, one file: folders list as JSON or HTML, files stream with
// Range, ETag and 304 support, Google-native files export to an open format.

export interface DriveR2Object {
  size: number;
  etag: string;
  httpEtag: string;
  uploaded?: Date;
  range?: { offset?: number; length?: number; suffix?: number };
  httpMetadata?: { contentType?: string; contentDisposition?: string };
  customMetadata?: Record<string, string>;
  body?: ReadableStream;
}

export interface DriveBucket {
  get(key: string, options?: { range?: Headers; onlyIf?: Headers }): Promise<DriveR2Object | null>;
  put(
    key: string,
    value: ReadableStream | ArrayBuffer,
    options?: { httpMetadata?: { contentType?: string; contentDisposition?: string }; customMetadata?: Record<string, string> },
  ): Promise<unknown>;
}

export interface DriveEnv {
  DRIVE_CACHE?: DriveBucket;
  /** Service-account key JSON (Wrangler secret). */
  GOOGLE_SA_KEY?: string;
  /** Drive folder id that /drive/ maps to. */
  DRIVE_ROOT_ID?: string;
  /** Access team origin, e.g. https://<team>.cloudflareaccess.com */
  ACCESS_TEAM_DOMAIN?: string;
  /** Access application AUD tag for fw.aphrody.com/drive. */
  ACCESS_AUD?: string;
}

export interface WaitUntilContext {
  waitUntil(promise: Promise<unknown>): void;
}

export interface DriveDeps {
  fetch: typeof fetch;
  now: () => number;
  /** FixedLengthStream exists on workerd only; tests and Bun fall back to buffering. */
  fixedLengthStream?: (length: number) => { readable: ReadableStream; writable: WritableStream };
}

export interface DriveIdentity {
  kind: "user" | "service";
  id: string;
}

interface DriveFile {
  id: string;
  name: string;
  mimeType: string;
  size?: string;
  md5Checksum?: string;
  modifiedTime?: string;
  parents?: string[];
}

export const DRIVE_PREFIX = "/drive";
const FOLDER = "application/vnd.google-apps.folder";
const API = "https://www.googleapis.com/drive/v3";
const SCOPE = "https://www.googleapis.com/auth/drive.readonly";
const FILE_FIELDS = "id,name,mimeType,size,md5Checksum,modifiedTime,parents";
const META_TTL_MS = 60_000;

/** Default export target per Google-native type; `?export=<mime>` overrides. */
export const EXPORT_DEFAULTS: Record<string, { mime: string; ext: string }> = {
  "application/vnd.google-apps.document": { mime: "text/markdown", ext: "md" },
  "application/vnd.google-apps.spreadsheet": { mime: "text/csv", ext: "csv" },
  "application/vnd.google-apps.presentation": { mime: "application/pdf", ext: "pdf" },
  "application/vnd.google-apps.drawing": { mime: "image/svg+xml", ext: "svg" },
  "application/vnd.google-apps.script": { mime: "application/vnd.google-apps.script+json", ext: "json" },
};

// Isolate-wide caches hold only shared, non-request data: verification keys, the service-account token
// and short-lived Drive metadata identical for every authorised caller.
const accessKeys = new Map<string, { keys: Map<string, CryptoKey>; expires: number }>();
const googleTokens = new Map<string, { token: string; expires: number }>();
const metaCache = new Map<string, { value: unknown; expires: number }>();

export function resetDriveCaches(): void {
  accessKeys.clear();
  googleTokens.clear();
  metaCache.clear();
}

function problem(status: number, error: string, extra: Record<string, unknown> = {}): Response {
  return new Response(JSON.stringify({ ok: false, error, ...extra }), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
  });
}

function b64urlBytes(input: string): Uint8Array<ArrayBuffer> {
  const b64 = input.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(input.length / 4) * 4, "=");
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
}

function b64urlJson(input: string): Record<string, unknown> {
  return JSON.parse(new TextDecoder().decode(b64urlBytes(input))) as Record<string, unknown>;
}

function b64urlEncode(bytes: Uint8Array | string): string {
  const raw = typeof bytes === "string" ? new TextEncoder().encode(bytes) : bytes;
  let s = "";
  for (const b of raw) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function cookie(request: Request, name: string): string | undefined {
  for (const part of (request.headers.get("cookie") ?? "").split(";")) {
    const [k, ...v] = part.trim().split("=");
    if (k === name) return v.join("=");
  }
  return undefined;
}

async function accessKeySet(team: string, deps: DriveDeps): Promise<Map<string, CryptoKey>> {
  const cached = accessKeys.get(team);
  if (cached && cached.expires > deps.now()) return cached.keys;
  const res = await deps.fetch(`${team}/cdn-cgi/access/certs`);
  if (!res.ok) throw new Error(`access certs: HTTP ${res.status}`);
  const body = (await res.json()) as { keys?: Array<JsonWebKey & { kid?: string }> };
  const keys = new Map<string, CryptoKey>();
  for (const jwk of body.keys ?? []) {
    if (!jwk.kid) continue;
    keys.set(jwk.kid, await crypto.subtle.importKey("jwk", jwk, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["verify"]));
  }
  accessKeys.set(team, { keys, expires: deps.now() + 3_600_000 });
  return keys;
}

/** Verifies the Cloudflare Access JWT (header or CF_Authorization cookie) against the team keys and AUD. */
export async function verifyAccess(request: Request, env: DriveEnv, deps: DriveDeps): Promise<DriveIdentity | Response> {
  const team = env.ACCESS_TEAM_DOMAIN?.replace(/\/+$/, "");
  if (!team || !env.ACCESS_AUD) return problem(503, "drive: Access is not configured (ACCESS_TEAM_DOMAIN, ACCESS_AUD)");
  const jwt = request.headers.get("cf-access-jwt-assertion") ?? cookie(request, "CF_Authorization");
  if (!jwt) return problem(401, "drive: sign in through Cloudflare Access", { login: `${team}` });
  const [h, p, s] = jwt.split(".");
  if (!h || !p || !s) return problem(401, "drive: malformed Access token");
  let header: Record<string, unknown>;
  let claims: Record<string, unknown>;
  try {
    header = b64urlJson(h);
    claims = b64urlJson(p);
  } catch {
    return problem(401, "drive: malformed Access token");
  }
  const key = (await accessKeySet(team, deps)).get(String(header.kid));
  if (header.alg !== "RS256" || !key) return problem(401, "drive: unknown Access signing key");
  const valid = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", key, b64urlBytes(s), new TextEncoder().encode(`${h}.${p}`));
  if (!valid) return problem(401, "drive: invalid Access signature");
  const now = deps.now() / 1000;
  const aud = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (claims.iss !== team || !aud.includes(env.ACCESS_AUD)) return problem(403, "drive: token was issued for another application");
  if (typeof claims.exp !== "number" || claims.exp < now || (typeof claims.nbf === "number" && claims.nbf > now + 60)) {
    return problem(401, "drive: Access token expired");
  }
  if (typeof claims.email === "string" && claims.email) return { kind: "user", id: claims.email };
  if (typeof claims.common_name === "string" && claims.common_name) return { kind: "service", id: claims.common_name };
  return problem(403, "drive: token carries no identity");
}

function pemToPkcs8(pem: string): ArrayBuffer {
  const body = pem.replace(/-----[^-]+-----/g, "").replace(/\s+/g, "");
  return Uint8Array.from(atob(body), (c) => c.charCodeAt(0)).buffer as ArrayBuffer;
}

/** OAuth 2.0 JWT-bearer grant for the service account (RFC 7523), cached until five minutes before expiry. */
export async function googleAccessToken(env: DriveEnv, deps: DriveDeps): Promise<string> {
  if (!env.GOOGLE_SA_KEY) throw new DriveConfigError("drive: GOOGLE_SA_KEY secret is not set");
  const sa = JSON.parse(env.GOOGLE_SA_KEY) as { client_email: string; private_key: string; token_uri?: string };
  const cached = googleTokens.get(sa.client_email);
  if (cached && cached.expires > deps.now()) return cached.token;
  const tokenUri = sa.token_uri ?? "https://oauth2.googleapis.com/token";
  const iat = Math.floor(deps.now() / 1000);
  const unsigned = `${b64urlEncode(JSON.stringify({ alg: "RS256", typ: "JWT" }))}.${b64urlEncode(
    JSON.stringify({ iss: sa.client_email, scope: SCOPE, aud: tokenUri, iat, exp: iat + 3600 }),
  )}`;
  const key = await crypto.subtle.importKey("pkcs8", pemToPkcs8(sa.private_key), { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["sign"]);
  const sig = new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(unsigned)));
  const res = await deps.fetch(tokenUri, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer", assertion: `${unsigned}.${b64urlEncode(sig)}` }),
  });
  if (!res.ok) throw new Error(`google token: HTTP ${res.status} ${await res.text()}`);
  const body = (await res.json()) as { access_token: string; expires_in: number };
  googleTokens.set(sa.client_email, { token: body.access_token, expires: deps.now() + (body.expires_in - 300) * 1000 });
  return body.access_token;
}

class DriveConfigError extends Error {}
class DriveNotFound extends Error {}

async function driveJson<T>(path: string, env: DriveEnv, deps: DriveDeps): Promise<T> {
  const hit = metaCache.get(path);
  if (hit && hit.expires > deps.now()) return hit.value as T;
  const res = await deps.fetch(`${API}${path}`, { headers: { authorization: `Bearer ${await googleAccessToken(env, deps)}` } });
  if (res.status === 404) throw new DriveNotFound(path);
  if (!res.ok) throw new Error(`drive ${path.split("?")[0]}: HTTP ${res.status} ${await res.text()}`);
  const value = (await res.json()) as T;
  metaCache.set(path, { value, expires: deps.now() + META_TTL_MS });
  return value;
}

const fileMeta = (id: string, env: DriveEnv, deps: DriveDeps) =>
  driveJson<DriveFile>(`/files/${encodeURIComponent(id)}?fields=${FILE_FIELDS}&supportsAllDrives=true`, env, deps);

function driveQuery(q: string): string {
  return `/files?q=${encodeURIComponent(q)}&fields=${encodeURIComponent(`nextPageToken,files(${FILE_FIELDS})`)}&pageSize=1000&orderBy=folder,name&supportsAllDrives=true&includeItemsFromAllDrives=true`;
}

const quote = (s: string) => `'${s.replace(/\\/g, "\\\\").replace(/'/g, "\\'")}'`;

async function listChildren(id: string, env: DriveEnv, deps: DriveDeps): Promise<DriveFile[]> {
  const out: DriveFile[] = [];
  let token = "";
  do {
    const page = await driveJson<{ files: DriveFile[]; nextPageToken?: string }>(
      `${driveQuery(`${quote(id)} in parents and trashed=false`)}${token ? `&pageToken=${encodeURIComponent(token)}` : ""}`,
      env,
      deps,
    );
    out.push(...page.files);
    token = page.nextPageToken ?? "";
  } while (token);
  return out;
}

async function resolvePath(segments: string[], env: DriveEnv, deps: DriveDeps): Promise<DriveFile> {
  let current = await fileMeta(env.DRIVE_ROOT_ID!, env, deps);
  for (const name of segments) {
    if (current.mimeType !== FOLDER) throw new DriveNotFound(name);
    const page = await driveJson<{ files: DriveFile[] }>(driveQuery(`${quote(current.id)} in parents and name=${quote(name)} and trashed=false`), env, deps);
    const next = page.files[0];
    if (!next) throw new DriveNotFound(name);
    current = next;
  }
  return current;
}

/** Ids reach only files under the root folder, never every file the service account could read. */
async function underRoot(file: DriveFile, env: DriveEnv, deps: DriveDeps): Promise<boolean> {
  let node = file;
  for (let depth = 0; depth < 32; depth++) {
    if (node.id === env.DRIVE_ROOT_ID) return true;
    const parent = node.parents?.[0];
    if (!parent) return false;
    node = await fileMeta(parent, env, deps);
  }
  return false;
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

function entryHref(base: string, f: DriveFile): string {
  return `${base}${encodeURIComponent(f.name)}${f.mimeType === FOLDER ? "/" : ""}`;
}

function listingResponse(request: Request, folder: DriveFile, path: string, children: DriveFile[], who: DriveIdentity): Response {
  const base = `${DRIVE_PREFIX}/${path ? `${path.split("/").map(encodeURIComponent).join("/")}/` : ""}`;
  const entries = children.map((f) => ({
    name: f.name,
    kind: f.mimeType === FOLDER ? "dir" : "file",
    mimeType: f.mimeType,
    size: f.size ? Number(f.size) : null,
    modified: f.modifiedTime ?? null,
    id: f.id,
    href: entryHref(base, f),
    export: EXPORT_DEFAULTS[f.mimeType]?.mime ?? null,
  }));
  const wantsHtml = (request.headers.get("accept") ?? "").includes("text/html") && new URL(request.url).searchParams.get("format") !== "json";
  if (!wantsHtml) {
    return new Response(JSON.stringify({ ok: true, path: `/${path}`, id: folder.id, name: folder.name, viewer: who, entries }), {
      headers: { "content-type": "application/json; charset=utf-8", "cache-control": "private, no-cache" },
    });
  }
  const rows = entries
    .map(
      (e) =>
        `<li><a href="${escapeHtml(e.href)}">${escapeHtml(e.name)}${e.kind === "dir" ? "/" : ""}</a><span>${
          e.kind === "dir" ? "Folder" : e.size === null ? escapeHtml(e.export ?? e.mimeType) : formatBytes(e.size)
        }</span></li>`,
    )
    .join("");
  const crumbs = [`<a href="${DRIVE_PREFIX}/">Drive</a>`]
    .concat(path ? path.split("/").map((seg, i, all) => `<a href="${DRIVE_PREFIX}/${all.slice(0, i + 1).map(encodeURIComponent).join("/")}/">${escapeHtml(seg)}</a>`) : [])
    .join(" / ");
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escapeHtml(folder.name)}: fw Drive</title><link rel="stylesheet" href="/app.css"><style>
main{max-width:56rem;margin:0 auto;padding:24px 16px;font-family:var(--font);color:var(--md-sys-color-on-surface)}
body{background:var(--md-sys-color-surface)}nav{color:var(--md-sys-color-on-surface-variant);margin-bottom:16px}
ul{list-style:none;padding:0;margin:0;border-radius:16px;overflow:hidden;background:var(--md-sys-color-surface-container)}
li{display:flex;justify-content:space-between;gap:16px;min-height:48px;align-items:center;padding:0 16px;border-bottom:1px solid var(--md-sys-color-outline-variant)}
li:last-child{border-bottom:0}a{color:var(--md-sys-color-primary)}a:focus-visible{outline:3px solid var(--md-sys-color-primary);outline-offset:2px}
li span{color:var(--md-sys-color-on-surface-variant);font-family:var(--font-code);font-size:14px;white-space:nowrap}p{color:var(--md-sys-color-on-surface-variant)}
</style></head><body><main><nav>${crumbs}</nav><h1>${escapeHtml(folder.name)}</h1>${
    entries.length ? `<ul>${rows}</ul>` : "<p>This folder is empty. Files added in Google Drive appear here within a minute.</p>"
  }<p>Signed in as ${escapeHtml(who.id)}. Add <code>?format=json</code> for the machine listing.</p></main></body></html>`;
  return new Response(html, { headers: { "content-type": "text/html; charset=utf-8", "cache-control": "private, no-cache" } });
}

function formatBytes(n: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${i === 0 ? v : v.toFixed(1)} ${units[i]}`;
}

function fileHeaders(object: Pick<DriveR2Object, "httpEtag" | "httpMetadata" | "uploaded">, cache: "hit" | "miss"): Headers {
  const h = new Headers();
  if (object.httpMetadata?.contentType) h.set("content-type", object.httpMetadata.contentType);
  if (object.httpMetadata?.contentDisposition) h.set("content-disposition", object.httpMetadata.contentDisposition);
  h.set("etag", object.httpEtag);
  if (object.uploaded) h.set("last-modified", object.uploaded.toUTCString());
  h.set("accept-ranges", "bytes");
  h.set("cache-control", "private, no-cache");
  h.set("x-content-type-options", "nosniff");
  // Drive content is untrusted: an HTML or SVG file must not run script on the fw.aphrody.com origin.
  h.set("content-security-policy", "sandbox; default-src 'none'; img-src data: blob:; media-src data: blob:; style-src 'unsafe-inline'");
  h.set("x-fw-drive-cache", cache);
  return h;
}

function fromR2(request: Request, object: DriveR2Object, cache: "hit" | "miss"): Response {
  const headers = fileHeaders(object, cache);
  if (!object.body) return new Response(null, { status: request.headers.has("if-none-match") || request.headers.has("if-modified-since") ? 304 : 412, headers });
  const body = request.method === "HEAD" ? null : object.body;
  const r = object.range;
  if (r && request.headers.has("range")) {
    const offset = r.suffix !== undefined ? object.size - r.suffix : (r.offset ?? 0);
    const length = r.suffix !== undefined ? r.suffix : (r.length ?? object.size - offset);
    headers.set("content-range", `bytes ${offset}-${offset + length - 1}/${object.size}`);
    headers.set("content-length", String(length));
    return new Response(body, { status: 206, headers });
  }
  headers.set("content-length", String(object.size));
  return new Response(body, { status: 200, headers });
}

function disposition(name: string): string {
  return `inline; filename*=UTF-8''${encodeURIComponent(name)}`;
}

async function serveFile(request: Request, file: DriveFile, env: DriveEnv, deps: DriveDeps, ctx?: WaitUntilContext): Promise<Response> {
  const bucket = env.DRIVE_CACHE!;
  const native = file.mimeType.startsWith("application/vnd.google-apps.");
  const requested = new URL(request.url).searchParams.get("export");
  const target = native ? (requested ? { mime: requested, ext: "" } : EXPORT_DEFAULTS[file.mimeType]) : undefined;
  if (native && !target) return problem(415, `drive: ${file.mimeType} has no export format; pass ?export=<mime>`);
  const version = native ? `${file.modifiedTime}/${target!.mime}` : (file.md5Checksum ?? file.modifiedTime ?? "0");
  const key = `drive/${file.id}/${version}`;
  const name = native && target!.ext ? `${file.name}.${target!.ext}` : file.name;
  const contentType = native ? target!.mime : file.mimeType || "application/octet-stream";
  const httpMetadata = { contentType, contentDisposition: disposition(name) };

  const cached = await bucket.get(key, { range: request.headers, onlyIf: request.headers });
  if (cached) return fromR2(request, cached, "hit");

  const token = await googleAccessToken(env, deps);
  const src = native
    ? `${API}/files/${encodeURIComponent(file.id)}/export?mimeType=${encodeURIComponent(target!.mime)}`
    : `${API}/files/${encodeURIComponent(file.id)}?alt=media&supportsAllDrives=true`;
  const customMetadata = { driveId: file.id, name: file.name, version };

  // A Range miss (video seek) goes straight to Drive and fills the cache in the background.
  if (request.headers.has("range") && !native) {
    const ranged = await deps.fetch(src, { headers: { authorization: `Bearer ${token}`, range: request.headers.get("range")! } });
    const fill = deps.fetch(src, { headers: { authorization: `Bearer ${token}` } }).then(async (full) => {
      if (full.ok && full.body) await putStream(bucket, key, full, file, deps, { httpMetadata, customMetadata });
    });
    if (ctx) ctx.waitUntil(fill);
    else await fill;
    const headers = fileHeaders({ httpEtag: `"${file.md5Checksum ?? version}"`, httpMetadata }, "miss");
    for (const h of ["content-range", "content-length"]) if (ranged.headers.get(h)) headers.set(h, ranged.headers.get(h)!);
    return new Response(request.method === "HEAD" ? null : ranged.body, { status: ranged.status, headers });
  }

  const upstream = await deps.fetch(src, { headers: { authorization: `Bearer ${token}` } });
  if (upstream.status === 404) return problem(404, "drive: file not found");
  if (!upstream.ok || !upstream.body) return problem(502, `drive: Google returned HTTP ${upstream.status}`, { detail: (await upstream.text()).slice(0, 500) });

  const size = Number(upstream.headers.get("content-length") ?? file.size ?? NaN);
  if (!native && Number.isFinite(size) && deps.fixedLengthStream) {
    // Stream to the caller and into R2 at once; R2 needs a known length, which FixedLengthStream supplies.
    const [toClient, toCache] = upstream.body.tee();
    const fixed = deps.fixedLengthStream(size);
    const fill = Promise.all([toCache.pipeTo(fixed.writable), bucket.put(key, fixed.readable, { httpMetadata, customMetadata })]);
    if (ctx) ctx.waitUntil(fill);
    else await fill;
    const headers = fileHeaders({ httpEtag: `"${file.md5Checksum ?? version}"`, httpMetadata }, "miss");
    headers.set("content-length", String(size));
    return new Response(request.method === "HEAD" ? null : toClient, { status: 200, headers });
  }

  // Exports (capped at 10 MB by Drive) and runtimes without FixedLengthStream: store, then serve from R2.
  await bucket.put(key, await upstream.arrayBuffer(), { httpMetadata, customMetadata });
  const stored = await bucket.get(key, { range: request.headers, onlyIf: request.headers });
  if (!stored) return problem(500, "drive: cache write did not persist");
  return fromR2(request, stored, "miss");
}

async function putStream(
  bucket: DriveBucket,
  key: string,
  res: Response,
  file: DriveFile,
  deps: DriveDeps,
  options: { httpMetadata: { contentType: string; contentDisposition: string }; customMetadata: Record<string, string> },
): Promise<void> {
  const size = Number(res.headers.get("content-length") ?? file.size ?? NaN);
  if (Number.isFinite(size) && deps.fixedLengthStream && res.body) {
    const fixed = deps.fixedLengthStream(size);
    await Promise.all([res.body.pipeTo(fixed.writable), bucket.put(key, fixed.readable, options)]);
    return;
  }
  await bucket.put(key, await res.arrayBuffer(), options);
}

export function defaultDriveDeps(): DriveDeps {
  const FLS = (globalThis as { FixedLengthStream?: new (n: number) => { readable: ReadableStream; writable: WritableStream } }).FixedLengthStream;
  return { fetch: (input, init) => fetch(input, init), now: () => Date.now(), fixedLengthStream: FLS ? (n) => new FLS(n) : undefined };
}

/** GET/HEAD /drive/<path>, /drive/_id/<fileId>, /drive/_whoami. */
export async function handleDrive(request: Request, env: DriveEnv, ctx?: WaitUntilContext, deps: DriveDeps = defaultDriveDeps()): Promise<Response> {
  if (request.method !== "GET" && request.method !== "HEAD") return problem(405, "drive: read-only, use GET or HEAD");
  const who = await verifyAccess(request, env, deps);
  if (who instanceof Response) return who;
  const url = new URL(request.url);
  const rest = url.pathname.slice(DRIVE_PREFIX.length).replace(/^\/+/, "");
  if (rest === "_whoami") return new Response(JSON.stringify({ ok: true, viewer: who }), { headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });
  if (!env.DRIVE_CACHE || !env.DRIVE_ROOT_ID) return problem(503, "drive: DRIVE_CACHE binding or DRIVE_ROOT_ID is not configured");
  try {
    if (rest.startsWith("_id/")) {
      const file = await fileMeta(decodeURIComponent(rest.slice(4)), env, deps);
      if (!(await underRoot(file, env, deps))) return problem(404, "drive: file not found");
      return file.mimeType === FOLDER ? listingResponse(request, file, "", await listChildren(file.id, env, deps), who) : serveFile(request, file, env, deps, ctx);
    }
    const trimmed = rest.replace(/\/+$/, "");
    const segments = trimmed ? trimmed.split("/").map((s) => decodeURIComponent(s)) : [];
    const file = await resolvePath(segments, env, deps);
    if (file.mimeType === FOLDER) {
      if (segments.length && !url.pathname.endsWith("/")) return Response.redirect(`${url.origin}${url.pathname}/${url.search}`, 308);
      return listingResponse(request, file, segments.join("/"), await listChildren(file.id, env, deps), who);
    }
    return await serveFile(request, file, env, deps, ctx);
  } catch (e) {
    if (e instanceof DriveNotFound) return problem(404, "drive: file not found", { path: `/${rest}` });
    if (e instanceof DriveConfigError) return problem(503, e.message);
    return problem(502, e instanceof Error ? e.message : String(e));
  }
}
