// End-to-end parity tests of the Rust Drive CDN (`/drive` of fw-edge) against `wrangler dev --local`, the
// scenarios of tests/fw_drive_cdn.test.ts. A Bun mock server stands in for Cloudflare Access (certs), Google's
// token endpoint and the Drive v3 API; the Worker reaches them through ACCESS_TEAM_DOMAIN, GOOGLE_API_BASE and
// the service-account key's token_uri. R2 is miniflare's.
// Run from crates/fw-edge after `worker-build --release`: `bun test tests/drive.test.ts`.
import { afterAll, beforeAll, describe, expect, setDefaultTimeout, test } from "bun:test";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { startEdge, type Edge } from "./harness.ts";

setDefaultTimeout(60_000);

const AUD = "aud-drive-test";
const ROOT = "root-id";
const SA_EMAIL = "drive-cdn@test.iam.gserviceaccount.com";
const enc = new TextEncoder();
const b64url = (b: Uint8Array | string) =>
  btoa(String.fromCharCode(...(typeof b === "string" ? enc.encode(b) : b))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const unb64url = (s: string) => Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));

const rsa = () => crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
const accessPair = await rsa();
const otherPair = await rsa();
const saPair = await rsa();
const accessJwk = { ...(await crypto.subtle.exportKey("jwk", accessPair.publicKey)), kid: "k1" };
const pkcs8 = new Uint8Array(await crypto.subtle.exportKey("pkcs8", saPair.privateKey));
const saPem = `-----BEGIN PRIVATE KEY-----\n${btoa(String.fromCharCode(...pkcs8)).replace(/(.{64})/g, "$1\n")}\n-----END PRIVATE KEY-----\n`;

async function jwt(claims: Record<string, unknown>, key = accessPair.privateKey, kid = "k1"): Promise<string> {
  const unsigned = `${b64url(JSON.stringify({ alg: "RS256", kid }))}.${b64url(JSON.stringify(claims))}`;
  return `${unsigned}.${b64url(new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, enc.encode(unsigned))))}`;
}

// ---- mock Access + Google --------------------------------------------------------------------------------

interface MockFile {
  id: string;
  name: string;
  mimeType: string;
  parents?: string[];
  size?: string;
  md5Checksum?: string;
  modifiedTime: string;
  bytes?: Uint8Array;
}

const FOLDER = "application/vnd.google-apps.folder";
const text = (s: string) => enc.encode(s);
const big = new Uint8Array(3 * 1024 * 1024 + 17).map((_, i) => (i * 31 + (i >> 8)) & 0xff);
const file = (f: MockFile): MockFile => ({ ...f, size: f.bytes ? String(f.bytes.length) : f.size });
const FILES: Record<string, MockFile> = Object.fromEntries(
  [
    { id: ROOT, name: "Aphrody", mimeType: FOLDER, modifiedTime: "2026-11-11T00:00:00Z" },
    { id: "sub", name: "art", mimeType: FOLDER, parents: [ROOT], modifiedTime: "2026-11-11T00:00:00Z" },
    { id: "empty", name: "void", mimeType: FOLDER, parents: [ROOT], modifiedTime: "2026-11-11T00:00:00Z" },
    { id: "many", name: "many", mimeType: FOLDER, parents: [ROOT], modifiedTime: "2026-11-11T00:00:00Z" },
    { id: "img", name: "logo.svg", mimeType: "image/svg+xml", parents: ["sub"], md5Checksum: "md5img", modifiedTime: "2026-11-12T00:00:00Z", bytes: text("0123456789") },
    { id: "clip", name: "clip.bin", mimeType: "application/octet-stream", parents: ["sub"], md5Checksum: "md5clip", modifiedTime: "2026-11-12T00:00:00Z", bytes: text("abcdefghij") },
    { id: "head", name: "head.bin", mimeType: "application/octet-stream", parents: ["sub"], md5Checksum: "md5head", modifiedTime: "2026-11-12T00:00:00Z", bytes: text("headhead01") },
    { id: "uni", name: "résumé final.txt", mimeType: "text/plain", parents: ["sub"], md5Checksum: "md5uni", modifiedTime: "2026-11-12T00:00:00Z", bytes: text("uni") },
    { id: "big", name: "big.bin", mimeType: "application/octet-stream", parents: [ROOT], md5Checksum: "md5big", modifiedTime: "2026-11-12T00:00:00Z", bytes: big },
    { id: "broken", name: "broken.bin", mimeType: "application/octet-stream", parents: [ROOT], md5Checksum: "md5broken", modifiedTime: "2026-11-12T00:00:00Z", size: "5" },
    { id: "doc", name: "Notes", mimeType: "application/vnd.google-apps.document", parents: [ROOT], modifiedTime: "2026-11-13T00:00:00Z" },
    { id: "form", name: "Survey", mimeType: "application/vnd.google-apps.form", parents: [ROOT], modifiedTime: "2026-11-14T00:00:00Z" },
    { id: "m1", name: "m1.txt", mimeType: "text/plain", parents: ["many"], md5Checksum: "m1", modifiedTime: "2026-11-12T00:00:00Z", bytes: text("1") },
    { id: "m2", name: "m2.txt", mimeType: "text/plain", parents: ["many"], md5Checksum: "m2", modifiedTime: "2026-11-12T00:00:00Z", bytes: text("2") },
    { id: "m3", name: "m3.txt", mimeType: "text/plain", parents: ["many"], md5Checksum: "m3", modifiedTime: "2026-11-12T00:00:00Z", bytes: text("3") },
    { id: "outside", name: "x", mimeType: "text/plain", parents: ["elsewhere"], md5Checksum: "m", modifiedTime: "2026-01-01T00:00:00Z", bytes: text("x") },
    { id: "elsewhere", name: "other", mimeType: FOLDER, modifiedTime: "2026-01-01T00:00:00Z" },
  ].map((f) => [f.id, file(f as MockFile)]),
);

const calls: string[] = [];
let certCalls = 0;
let tokenCalls = 0;
const publicMeta = ({ bytes: _b, ...meta }: MockFile) => meta;

function ranged(bytes: Uint8Array, range: string | null): Response {
  const m = range && /^bytes=(\d*)-(\d*)$/.exec(range);
  if (!m) return new Response(bytes, { headers: { "content-length": String(bytes.length) } });
  const start = m[1] === "" ? Math.max(0, bytes.length - Number(m[2])) : Number(m[1]);
  const end = m[1] === "" || m[2] === "" ? bytes.length - 1 : Math.min(Number(m[2]), bytes.length - 1);
  const slice = bytes.slice(start, end + 1);
  return new Response(slice, { status: 206, headers: { "content-range": `bytes ${start}-${end}/${bytes.length}`, "content-length": String(slice.length) } });
}

const unquote = (s: string) => s.replace(/\\(.)/g, "$1");

const mock = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  async fetch(req) {
    const url = new URL(req.url);
    if (url.pathname === "/cdn-cgi/access/certs") {
      certCalls++;
      return Response.json({ keys: [accessJwk], public_cert: {} });
    }
    if (url.pathname === "/token") {
      tokenCalls++;
      const form = new URLSearchParams(await req.text());
      if (form.get("grant_type") !== "urn:ietf:params:oauth:grant-type:jwt-bearer") return new Response("bad grant", { status: 400 });
      const [h, p, s] = form.get("assertion")!.split(".");
      const claims = JSON.parse(new TextDecoder().decode(unb64url(p)));
      const sigOk = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", saPair.publicKey, unb64url(s), enc.encode(`${h}.${p}`));
      const good =
        sigOk &&
        JSON.parse(new TextDecoder().decode(unb64url(h))).alg === "RS256" &&
        claims.iss === SA_EMAIL &&
        claims.scope === "https://www.googleapis.com/auth/drive.readonly" &&
        claims.aud === `${mockUrl}/token` &&
        claims.exp - claims.iat === 3600;
      return good ? Response.json({ access_token: "ya29.test", expires_in: 3600 }) : new Response("bad assertion", { status: 400 });
    }
    if (!url.pathname.startsWith("/drive/v3/")) return new Response("unexpected", { status: 500 });
    calls.push(`${req.method} ${url.pathname}${url.search}${req.headers.get("range") ? ` [${req.headers.get("range")}]` : ""}`);
    if (req.headers.get("authorization") !== "Bearer ya29.test") return new Response("unauthorized", { status: 401 });
    const exportMatch = /^\/drive\/v3\/files\/([^/]+)\/export$/.exec(url.pathname);
    if (exportMatch) {
      const f = FILES[decodeURIComponent(exportMatch[1])];
      return new Response(`# ${f.name}`, { headers: { "content-type": url.searchParams.get("mimeType")! } });
    }
    const fileMatch = /^\/drive\/v3\/files\/([^/]+)$/.exec(url.pathname);
    if (fileMatch) {
      const f = FILES[decodeURIComponent(fileMatch[1])];
      if (!f) return new Response("not found", { status: 404 });
      if (url.searchParams.get("alt") === "media") {
        if (f.id === "broken") return new Response("upstream exploded", { status: 500 });
        return ranged(f.bytes!, req.headers.get("range"));
      }
      return Response.json(publicMeta(f));
    }
    if (url.pathname === "/drive/v3/files") {
      const q = url.searchParams.get("q")!;
      const parent = /^'([^']+)' in parents/.exec(q)![1];
      const name = /name='((?:[^'\\]|\\.)*)'/.exec(q)?.[1];
      const all = Object.values(FILES)
        .filter((f) => f.parents?.includes(parent) && (name === undefined || f.name === unquote(name)))
        .sort((a, b) => Number(b.mimeType === FOLDER) - Number(a.mimeType === FOLDER) || a.name.localeCompare(b.name));
      // The "many" folder pages two entries at a time to exercise nextPageToken.
      const page = parent === "many" ? Number(url.searchParams.get("pageToken") ?? 0) : 0;
      const size = parent === "many" ? 2 : all.length;
      const files = all.slice(page * size, page * size + size).map(publicMeta);
      return Response.json(parent === "many" && (page + 1) * size < all.length ? { files, nextPageToken: String(page + 1) } : { files });
    }
    return new Response("unexpected", { status: 500 });
  },
});
const mockUrl = `http://127.0.0.1:${mock.port}`;
const SA_KEY = JSON.stringify({ client_email: SA_EMAIL, private_key: saPem, token_uri: `${mockUrl}/token` });

const now = () => Math.floor(Date.now() / 1000);
const userClaims = () => ({ iss: mockUrl, aud: [AUD], exp: now() + 600, email: "person@example.com" });

let edge: Edge;
let state: string;
const base = () => `${edge.url}/drive`;
const fetchDrive = async (path: string, init: RequestInit & { claims?: Record<string, unknown>; anonymous?: boolean } = {}) => {
  const { claims, anonymous, headers, ...rest } = init;
  const h = new Headers(headers);
  if (!anonymous && !h.has("cookie")) h.set("cf-access-jwt-assertion", await jwt(claims ?? userClaims()));
  return fetch(`${edge.url}${path}`, { redirect: "manual", ...rest, headers: h });
};
const sha = async (buf: ArrayBuffer | Uint8Array) => b64url(new Uint8Array(await crypto.subtle.digest("SHA-256", buf)));

/** The cache fill runs in waitUntil after the response: poll until R2 answers with a hit. */
async function untilHit(path: string, init: RequestInit = {}): Promise<Response> {
  for (let i = 0; i < 100; i++) {
    const res = await fetchDrive(path, init);
    if (res.headers.get("x-fw-drive-cache") === "hit") return res;
    await res.arrayBuffer();
    await Bun.sleep(100);
  }
  throw new Error(`${path} never became a cache hit`);
}

const mediaCalls = (id: string) => calls.filter((c) => c.includes(`/drive/v3/files/${id}?alt=media`));

beforeAll(async () => {
  state = await mkdtemp(join(tmpdir(), "fw-edge-drive-"));
  edge = await startEdge({
    persistTo: state,
    vars: { ACCESS_TEAM_DOMAIN: mockUrl, ACCESS_AUD: AUD, GOOGLE_API_BASE: mockUrl, DRIVE_ROOT_ID: ROOT, GOOGLE_SA_KEY: SA_KEY },
  });
}, 300_000);

afterAll(async () => {
  await edge?.stop();
  await rm(state, { recursive: true, force: true });
});

describe("drive CDN: Cloudflare Access", () => {
  test("rejects missing, foreign-key, wrong-audience, wrong-issuer and expired tokens", async () => {
    const missing = await fetchDrive("/drive/", { anonymous: true });
    expect(missing.status).toBe(401);
    expect(await missing.json()).toEqual({ ok: false, error: "drive: sign in through Cloudflare Access", login: mockUrl });
    expect(missing.headers.get("cache-control")).toBe("no-store");
    expect(missing.headers.get("content-type")).toBe("application/json; charset=utf-8");

    const forged = await fetch(`${base()}/`, { headers: { "cf-access-jwt-assertion": await jwt(userClaims(), otherPair.privateKey) } });
    expect(forged.status).toBe(401);
    expect((await forged.json()).error).toBe("drive: invalid Access signature");
    const unknownKid = await fetch(`${base()}/`, { headers: { "cf-access-jwt-assertion": await jwt(userClaims(), accessPair.privateKey, "nope") } });
    expect((await unknownKid.json()).error).toBe("drive: unknown Access signing key");
    const malformed = await fetch(`${base()}/`, { headers: { "cf-access-jwt-assertion": "not-a-jwt" } });
    expect(malformed.status).toBe(401);
    expect((await malformed.json()).error).toBe("drive: malformed Access token");
    const junk = await fetch(`${base()}/`, { headers: { "cf-access-jwt-assertion": "!!!.@@@.###" } });
    expect((await junk.json()).error).toBe("drive: malformed Access token");

    const wrongAud = await fetchDrive("/drive/", { claims: { ...userClaims(), aud: ["other"] } });
    expect(wrongAud.status).toBe(403);
    expect((await wrongAud.json()).error).toBe("drive: token was issued for another application");
    expect((await fetchDrive("/drive/", { claims: { ...userClaims(), iss: "https://evil.example" } })).status).toBe(403);
    expect((await fetchDrive("/drive/", { claims: { ...userClaims(), aud: AUD } })).status).toBe(200);
    const expired = await fetchDrive("/drive/", { claims: { ...userClaims(), exp: now() - 1 } });
    expect(expired.status).toBe(401);
    expect((await expired.json()).error).toBe("drive: Access token expired");
    expect((await fetchDrive("/drive/", { claims: { ...userClaims(), nbf: now() + 3600 } })).status).toBe(401);
    expect((await fetchDrive("/drive/", { claims: { iss: mockUrl, aud: [AUD], exp: now() + 60 } })).status).toBe(403);
  });

  test("people come in with an email, agents with a service-token client id", async () => {
    const user = await (await fetchDrive("/drive/_whoami")).json();
    expect(user).toEqual({ ok: true, viewer: { kind: "user", id: "person@example.com" } });
    const res = await fetchDrive("/drive/_whoami", { claims: { iss: mockUrl, aud: AUD, exp: now() + 60, common_name: "abc.access" } });
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(await res.json()).toEqual({ ok: true, viewer: { kind: "service", id: "abc.access" } });
  });

  test("the CF_Authorization cookie works for browsers, and the Access certs are cached", async () => {
    const res = await fetchDrive("/drive/_whoami", { headers: { cookie: `a=b; CF_Authorization=${await jwt(userClaims())}` } });
    expect(res.status).toBe(200);
    expect((await res.json()).viewer.id).toBe("person@example.com");
    const before = certCalls;
    for (let i = 0; i < 3; i++) await (await fetchDrive("/drive/_whoami")).arrayBuffer();
    expect(certCalls).toBe(before);
    expect(certCalls).toBeGreaterThan(0);
  });

  test("writes are refused before anything else", async () => {
    for (const method of ["PUT", "POST", "PATCH", "DELETE"]) {
      const res = await fetch(`${base()}/x`, { method, body: "x" });
      expect(res.status).toBe(405);
      expect(await res.json()).toEqual({ ok: false, error: "drive: read-only, use GET or HEAD" });
    }
  });
});

describe("drive CDN: folders", () => {
  test("root lists as JSON for agents", async () => {
    const res = await fetchDrive("/drive/");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe("application/json; charset=utf-8");
    expect(res.headers.get("cache-control")).toBe("private, no-cache");
    const body = await res.json();
    expect(body).toMatchObject({ ok: true, path: "/", id: ROOT, name: "Aphrody", viewer: { kind: "user", id: "person@example.com" } });
    expect(body.entries.map((e: { name: string; kind: string; href: string; export: string | null; size: number | null }) => [e.name, e.kind, e.href, e.export, e.size])).toEqual([
      ["art", "dir", "/drive/art/", null, null],
      ["many", "dir", "/drive/many/", null, null],
      ["void", "dir", "/drive/void/", null, null],
      ["big.bin", "file", "/drive/big.bin", null, big.length],
      ["broken.bin", "file", "/drive/broken.bin", null, 5],
      ["Notes", "file", "/drive/Notes", "text/markdown", null],
      ["Survey", "file", "/drive/Survey", null, null],
    ]);
  });

  test("browsers get HTML, ?format=json overrides", async () => {
    const html = await fetchDrive("/drive/art/", { headers: { accept: "text/html,application/xhtml+xml" } });
    expect(html.headers.get("content-type")).toBe("text/html; charset=utf-8");
    const body = await html.text();
    expect(body).toContain('<nav><a href="/drive/">Drive</a> / <a href="/drive/art/">art</a></nav><h1>art</h1>');
    expect(body).toContain('<a href="/drive/art/r%C3%A9sum%C3%A9%20final.txt">résumé final.txt</a><span>3 B</span>');
    expect(body).toContain("Signed in as person@example.com.");
    const json = await fetchDrive("/drive/art/?format=json", { headers: { accept: "text/html" } });
    expect(json.headers.get("content-type")).toBe("application/json; charset=utf-8");
    expect((await json.json()).path).toBe("/art");
    const empty = await (await fetchDrive("/drive/void/", { headers: { accept: "text/html" } })).text();
    expect(empty).toContain("<p>This folder is empty. Files added in Google Drive appear here within a minute.</p>");
  });

  test("a folder path without a trailing slash redirects", async () => {
    const res = await fetchDrive("/drive/art");
    expect(res.status).toBe(308);
    expect(res.headers.get("location")).toBe(`${edge.url}/drive/art/`);
    const withQuery = await fetchDrive("/drive/art?format=json&x=1");
    expect(withQuery.headers.get("location")).toBe(`${edge.url}/drive/art/?format=json&x=1`);
  });

  test("every Drive page of a big folder is listed", async () => {
    const body = await (await fetchDrive("/drive/many/")).json();
    expect(body.entries.map((e: { name: string }) => e.name)).toEqual(["m1.txt", "m2.txt", "m3.txt"]);
  });

  test("ids resolve only inside the shared root", async () => {
    const ok = await fetchDrive("/drive/_id/img");
    expect(ok.status).toBe(200);
    expect(await ok.text()).toBe("0123456789");
    const folder = await (await fetchDrive("/drive/_id/sub")).json();
    expect(folder).toMatchObject({ ok: true, path: "/", id: "sub", name: "art" });
    expect(folder.entries.map((e: { href: string }) => e.href)).toContain("/drive/logo.svg");
    const outside = await fetchDrive("/drive/_id/outside");
    expect(outside.status).toBe(404);
    expect(await outside.json()).toEqual({ ok: false, error: "drive: file not found" });
    expect((await fetchDrive("/drive/_id/elsewhere")).status).toBe(404);
    expect((await fetchDrive("/drive/_id/nope")).status).toBe(404);
    const missing = await fetchDrive("/drive/missing.txt");
    expect(missing.status).toBe(404);
    expect(await missing.json()).toEqual({ ok: false, error: "drive: file not found", path: "/missing.txt" });
    expect((await fetchDrive("/drive/logo.svg/deeper")).status).toBe(404);
  });
});

describe("drive CDN: files", () => {
  test("first read fetches Drive and fills R2, second read is a cache hit", async () => {
    const before = mediaCalls("img").length;
    const miss = await fetchDrive("/drive/art/logo.svg");
    expect(miss.status).toBe(200);
    expect(miss.headers.get("x-fw-drive-cache")).toBe("miss");
    expect(miss.headers.get("content-length")).toBe("10");
    expect(miss.headers.get("etag")).toBe('"md5img"');
    expect(miss.headers.get("content-type")).toBe("image/svg+xml");
    expect(miss.headers.get("content-disposition")).toBe("inline; filename*=UTF-8''logo.svg");
    expect(await miss.text()).toBe("0123456789");

    const hit = await untilHit("/drive/art/logo.svg");
    expect(hit.status).toBe(200);
    expect(await hit.text()).toBe("0123456789");
    expect(mediaCalls("img").length - before).toBe(1);
    expect(hit.headers.get("content-security-policy")).toStartWith("sandbox; default-src 'none'");
    expect(hit.headers.get("content-type")).toBe("image/svg+xml");
    expect(hit.headers.get("content-disposition")).toBe("inline; filename*=UTF-8''logo.svg");
    expect(hit.headers.get("content-length")).toBe("10");
    expect(hit.headers.get("accept-ranges")).toBe("bytes");
    expect(hit.headers.get("cache-control")).toBe("private, no-cache");
    expect(hit.headers.get("x-content-type-options")).toBe("nosniff");
    expect(hit.headers.get("last-modified")).toMatch(/^[A-Z][a-z]{2}, \d{2} [A-Z][a-z]{2} \d{4} \d{2}:\d{2}:\d{2} GMT$/);
    expect(hit.headers.get("etag")).toMatch(/^"[0-9a-f]+"$/);
    expect(mediaCalls("img").length - before).toBe(1);
  });

  test("a miss streams the full body to the caller and into R2 at once", async () => {
    const before = mediaCalls("big").length;
    const miss = await fetchDrive("/drive/big.bin");
    expect(miss.headers.get("x-fw-drive-cache")).toBe("miss");
    expect(miss.headers.get("content-length")).toBe(String(big.length));
    const bytes = await miss.arrayBuffer();
    expect(bytes.byteLength).toBe(big.length);
    expect(await sha(bytes)).toBe(await sha(big));
    const hit = await untilHit("/drive/big.bin");
    expect(await sha(await hit.arrayBuffer())).toBe(await sha(big));
    expect(mediaCalls("big").length - before).toBe(1);
    const tail = await fetchDrive("/drive/big.bin", { headers: { range: `bytes=-1000` } });
    expect(tail.status).toBe(206);
    expect(tail.headers.get("content-range")).toBe(`bytes ${big.length - 1000}-${big.length - 1}/${big.length}`);
    expect(await sha(await tail.arrayBuffer())).toBe(await sha(big.slice(big.length - 1000)));
  });

  test("cache hits honour Range", async () => {
    await (await untilHit("/drive/art/logo.svg")).arrayBuffer();
    const part = await fetchDrive("/drive/art/logo.svg", { headers: { range: "bytes=2-4" } });
    expect(part.status).toBe(206);
    expect(part.headers.get("content-range")).toBe("bytes 2-4/10");
    expect(part.headers.get("content-length")).toBe("3");
    expect(part.headers.get("x-fw-drive-cache")).toBe("hit");
    expect(await part.text()).toBe("234");
    const suffix = await fetchDrive("/drive/art/logo.svg", { headers: { range: "bytes=-3" } });
    expect(suffix.headers.get("content-range")).toBe("bytes 7-9/10");
    expect(await suffix.text()).toBe("789");
    const open = await fetchDrive("/drive/art/logo.svg", { headers: { range: "bytes=8-" } });
    expect(open.headers.get("content-range")).toBe("bytes 8-9/10");
    expect(await open.text()).toBe("89");
    const clamped = await fetchDrive("/drive/art/logo.svg", { headers: { range: "bytes=8-999" } });
    expect(clamped.status).toBe(206);
    expect(clamped.headers.get("content-range")).toBe("bytes 8-9/10");
    expect(await clamped.text()).toBe("89");
    const multi = await fetchDrive("/drive/art/logo.svg", { headers: { range: "bytes=0-1,4-5" } });
    expect(multi.status).toBe(200);
    expect(await multi.text()).toBe("0123456789");
    const beyond = await fetchDrive("/drive/art/logo.svg", { headers: { range: "bytes=50-60" } });
    expect(beyond.status).toBe(416);
    expect(beyond.headers.get("content-range")).toBe("bytes */10");
    await beyond.arrayBuffer();
  });

  test("cache hits honour If-None-Match, If-Match and the date conditions", async () => {
    const first = await untilHit("/drive/art/logo.svg");
    await first.arrayBuffer();
    const etag = first.headers.get("etag")!;
    const lastModified = first.headers.get("last-modified")!;
    const same = await fetchDrive("/drive/art/logo.svg", { headers: { "if-none-match": etag } });
    expect(same.status).toBe(304);
    expect(await same.text()).toBe("");
    expect(same.headers.get("etag")).toBe(etag);
    expect(same.headers.get("x-fw-drive-cache")).toBe("hit");
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-none-match": `W/${etag}` } })).status).toBe(304);
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-none-match": `"other", ${etag}` } })).status).toBe(304);
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-none-match": "*" } })).status).toBe(304);
    const different = await fetchDrive("/drive/art/logo.svg", { headers: { "if-none-match": '"other"' } });
    expect(different.status).toBe(200);
    expect(await different.text()).toBe("0123456789");
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-match": etag } })).status).toBe(200);
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-match": '"other"' } })).status).toBe(412);
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-match": '"a", "b"' } })).status).toBe(412);
    // Last-Modified has second granularity, the upload time milliseconds: sending it back must still be a 304
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-modified-since": lastModified } })).status).toBe(304);
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-modified-since": "Thu, 01 Jan 2015 00:00:00 GMT" } })).status).toBe(200);
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-unmodified-since": "Thu, 01 Jan 2015 00:00:00 GMT" } })).status).toBe(412);
    expect((await fetchDrive("/drive/art/logo.svg", { headers: { "if-unmodified-since": lastModified } })).status).toBe(200);
    const ranged = await fetchDrive("/drive/art/logo.svg", { headers: { range: "bytes=2-4", "if-none-match": etag } });
    expect(ranged.status).toBe(304);
  });

  test("HEAD answers without a body, on a hit and on a miss", async () => {
    const miss = await fetchDrive("/drive/art/head.bin", { method: "HEAD" });
    expect(miss.status).toBe(200);
    expect(miss.headers.get("x-fw-drive-cache")).toBe("miss");
    expect(miss.headers.get("content-length")).toBe("10");
    expect(await miss.text()).toBe("");
    const hit = await untilHit("/drive/art/head.bin", { method: "HEAD" });
    expect(hit.headers.get("content-length")).toBe("10");
    expect(await hit.text()).toBe("");
    const full = await fetchDrive("/drive/art/head.bin");
    expect(await full.text()).toBe("headhead01");
    const folder = await fetchDrive("/drive/art/", { method: "HEAD" });
    expect(folder.status).toBe(200);
  });

  test("a Range miss is served by Drive while the full file fills R2 in the background", async () => {
    const res = await fetchDrive("/drive/art/clip.bin", { headers: { range: "bytes=2-4" } });
    expect(res.status).toBe(206);
    expect(res.headers.get("x-fw-drive-cache")).toBe("miss");
    expect(res.headers.get("content-range")).toBe("bytes 2-4/10");
    expect(res.headers.get("content-length")).toBe("3");
    expect(res.headers.get("etag")).toBe('"md5clip"');
    expect(await res.text()).toBe("cde");
    const hit = await untilHit("/drive/art/clip.bin");
    expect(await hit.text()).toBe("abcdefghij");
    expect(calls.filter((c) => c.includes("/drive/v3/files/clip?alt=media") && c.includes("[bytes=2-4]"))).toHaveLength(1);
    expect(mediaCalls("clip")).toHaveLength(2);
  });

  test("names with spaces and non-ASCII letters round-trip through paths and Content-Disposition", async () => {
    const res = await fetchDrive("/drive/art/r%C3%A9sum%C3%A9%20final.txt");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-disposition")).toBe("inline; filename*=UTF-8''r%C3%A9sum%C3%A9%20final.txt");
    expect(await res.text()).toBe("uni");
  });

  test("Google Docs export to Markdown by default, any format on request", async () => {
    const md = await fetchDrive("/drive/Notes");
    expect(md.status).toBe(200);
    expect(md.headers.get("content-type")).toBe("text/markdown");
    expect(md.headers.get("content-disposition")).toBe("inline; filename*=UTF-8''Notes.md");
    expect(md.headers.get("x-fw-drive-cache")).toBe("miss");
    expect(await md.text()).toBe("# Notes");
    const again = await fetchDrive("/drive/Notes");
    expect(again.headers.get("x-fw-drive-cache")).toBe("hit");
    expect(await again.text()).toBe("# Notes");
    const pdf = await fetchDrive("/drive/Notes?export=application/pdf");
    expect(pdf.headers.get("content-type")).toBe("application/pdf");
    expect(pdf.headers.get("content-disposition")).toBe("inline; filename*=UTF-8''Notes");
    expect(pdf.headers.get("x-fw-drive-cache")).toBe("miss");
    expect(await pdf.text()).toBe("# Notes");
    expect(calls.filter((c) => c.includes("/drive/v3/files/doc/export?mimeType=text%2Fmarkdown"))).toHaveLength(1);
    expect(calls.filter((c) => c.includes("/drive/v3/files/doc/export?mimeType=application%2Fpdf"))).toHaveLength(1);
    const partial = await fetchDrive("/drive/Notes", { headers: { range: "bytes=2-" } });
    expect(partial.status).toBe(206);
    expect(await partial.text()).toBe("Notes");
  });

  test("Google-native types without a default export need ?export=", async () => {
    const none = await fetchDrive("/drive/Survey");
    expect(none.status).toBe(415);
    expect(await none.json()).toEqual({ ok: false, error: "drive: application/vnd.google-apps.form has no export format; pass ?export=<mime>" });
    expect((await fetchDrive("/drive/Survey?export=application/pdf")).status).toBe(200);
  });

  test("Google failures surface as 502 problems", async () => {
    const res = await fetchDrive("/drive/broken.bin");
    expect(res.status).toBe(502);
    expect(await res.json()).toEqual({ ok: false, error: "drive: Google returned HTTP 500", detail: "upstream exploded" });
  });

  test("the service-account token is fetched once and reused", () => {
    expect(tokenCalls).toBe(1);
  });
});

describe("drive CDN: byte parity with cloudflare/drive.ts", () => {
  const tsPath = fileURLToPath(new URL("../../../cloudflare/drive.ts", import.meta.url));
  const tsTest = existsSync(tsPath) ? test : test.skip;
  const ts = async () => (await import(tsPath)) as typeof import("../../../cloudflare/drive.ts");

  async function both(path: string, headers: Record<string, string> = {}, claims?: Record<string, unknown>) {
    const mod = await ts();
    mod.resetDriveCaches();
    const token = claims === null ? undefined : await jwt(claims ?? userClaims());
    const h = { ...(token ? { "cf-access-jwt-assertion": token } : {}), ...headers };
    const rewrite = (input: RequestInfo | URL) => String(input).replace("https://www.googleapis.com", mockUrl);
    const deps = { fetch: (input: RequestInfo | URL, init?: RequestInit) => fetch(rewrite(input), init), now: () => Date.now() };
    const env = { DRIVE_CACHE: {} as never, GOOGLE_SA_KEY: SA_KEY, DRIVE_ROOT_ID: ROOT, ACCESS_TEAM_DOMAIN: mockUrl, ACCESS_AUD: AUD };
    const expected = await mod.handleDrive(new Request(`https://fw.aphrody.com${path}`, { headers: h }), env, undefined, deps);
    const actual = await fetch(`${edge.url}${path}`, { headers: h, redirect: "manual" });
    return { expected, actual };
  }

  for (const [name, path, headers] of [
    ["root HTML", "/drive/", { accept: "text/html" }],
    ["nested HTML with encoded names", "/drive/art/", { accept: "text/html" }],
    ["empty folder HTML", "/drive/void/", { accept: "text/html" }],
    ["root JSON", "/drive/", {}],
    ["nested JSON", "/drive/art/", {}],
    ["id JSON", "/drive/_id/sub", {}],
    ["paged JSON", "/drive/many/", {}],
    ["404 problem", "/drive/missing.txt", {}],
    ["415 problem", "/drive/Survey", {}],
    ["id 404 problem", "/drive/_id/outside", {}],
  ] as const) {
    tsTest(`${name} is byte-identical`, async () => {
      const { expected, actual } = await both(path, headers);
      expect(actual.status).toBe(expected.status);
      expect(actual.headers.get("content-type")).toBe(expected.headers.get("content-type"));
      expect(actual.headers.get("cache-control")).toBe(expected.headers.get("cache-control"));
      expect(await actual.text()).toBe(await expected.text());
    });
  }

  tsTest("401 problem is byte-identical", async () => {
    const { expected, actual } = await both("/drive/", {}, null as never);
    expect(actual.status).toBe(expected.status);
    expect(await actual.text()).toBe(await expected.text());
  });

  tsTest("403 problem is byte-identical", async () => {
    const { expected, actual } = await both("/drive/", {}, { ...userClaims(), aud: ["other"] });
    expect(actual.status).toBe(expected.status);
    expect(await actual.text()).toBe(await expected.text());
  });
});

describe("drive CDN: unconfigured Access fails closed", () => {
  let bare: Edge;
  let bareState: string;
  beforeAll(async () => {
    await edge.stop();
    bareState = await mkdtemp(join(tmpdir(), "fw-edge-drive-bare-"));
    bare = await startEdge({ persistTo: bareState, vars: { ACCESS_TEAM_DOMAIN: "", ACCESS_AUD: "" } });
  }, 300_000);
  afterAll(async () => {
    await bare?.stop();
    await rm(bareState, { recursive: true, force: true });
  });

  test("fw-edge mounts the drive CDN at /drive and answers 503", async () => {
    const res = await fetch(`${bare.url}/drive/_whoami`);
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({ ok: false, error: "drive: Access is not configured (ACCESS_TEAM_DOMAIN, ACCESS_AUD)" });
    expect((await fetch(`${bare.url}/drive`)).status).toBe(503);
  });
});
