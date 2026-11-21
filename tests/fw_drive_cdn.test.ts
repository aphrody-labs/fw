import { beforeEach, describe, expect, test } from "bun:test";
import { handleDrive, resetDriveCaches, type DriveBucket, type DriveDeps, type DriveEnv, type DriveR2Object } from "../cloudflare/drive.ts";
import worker from "../cloudflare/worker.ts";

const TEAM = "https://team.cloudflareaccess.com";
const AUD = "aud-drive";
const ROOT = "root-id";
const enc = new TextEncoder();
const b64url = (b: Uint8Array | string) =>
  btoa(String.fromCharCode(...(typeof b === "string" ? enc.encode(b) : b))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

const rsa = () => crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
const accessPair = await rsa();
const otherPair = await rsa();
const saPair = await rsa();
const accessJwk = { ...(await crypto.subtle.exportKey("jwk", accessPair.publicKey)), kid: "k1" };
const pkcs8 = new Uint8Array(await crypto.subtle.exportKey("pkcs8", saPair.privateKey));
const saPem = `-----BEGIN PRIVATE KEY-----\n${btoa(String.fromCharCode(...pkcs8)).replace(/(.{64})/g, "$1\n")}\n-----END PRIVATE KEY-----\n`;
const SA_KEY = JSON.stringify({ client_email: "drive-cdn@test.iam.gserviceaccount.com", private_key: saPem, token_uri: "https://oauth2.googleapis.com/token" });

async function jwt(claims: Record<string, unknown>, key = accessPair.privateKey, kid = "k1"): Promise<string> {
  const unsigned = `${b64url(JSON.stringify({ alg: "RS256", kid }))}.${b64url(JSON.stringify(claims))}`;
  return `${unsigned}.${b64url(new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, enc.encode(unsigned))))}`;
}
const now = 1_800_000_000_000;
const userClaims = { iss: TEAM, aud: [AUD], exp: now / 1000 + 600, email: "person@example.com" };

const FILES: Record<string, { id: string; name: string; mimeType: string; parents?: string[]; size?: string; md5Checksum?: string; modifiedTime: string; bytes?: string }> = {
  [ROOT]: { id: ROOT, name: "Aphrody", mimeType: "application/vnd.google-apps.folder", modifiedTime: "2026-11-11T00:00:00Z" },
  sub: { id: "sub", name: "art", mimeType: "application/vnd.google-apps.folder", parents: [ROOT], modifiedTime: "2026-11-11T00:00:00Z" },
  img: { id: "img", name: "logo.svg", mimeType: "image/svg+xml", parents: ["sub"], size: "10", md5Checksum: "md5img", modifiedTime: "2026-11-12T00:00:00Z", bytes: "0123456789" },
  doc: { id: "doc", name: "Notes", mimeType: "application/vnd.google-apps.document", parents: [ROOT], modifiedTime: "2026-11-13T00:00:00Z" },
  outside: { id: "outside", name: "x", mimeType: "text/plain", parents: ["elsewhere"], size: "1", md5Checksum: "m", modifiedTime: "2026-01-01T00:00:00Z", bytes: "x" },
  elsewhere: { id: "elsewhere", name: "other", mimeType: "application/vnd.google-apps.folder", modifiedTime: "2026-01-01T00:00:00Z" },
};

function memoryBucket(): DriveBucket & { store: Map<string, { bytes: Uint8Array; meta: DriveR2Object["httpMetadata"] }> } {
  const store = new Map<string, { bytes: Uint8Array; meta: DriveR2Object["httpMetadata"] }>();
  return {
    store,
    async get(key, options) {
      const hit = store.get(key);
      if (!hit) return null;
      const etag = `"etag-${key}"`;
      const base = { size: hit.bytes.length, etag: `etag-${key}`, httpEtag: etag, httpMetadata: hit.meta };
      if (options?.onlyIf?.get("if-none-match") === etag) return base;
      const m = /bytes=(\d+)-(\d*)/.exec(options?.range?.get("range") ?? "");
      if (m) {
        const offset = Number(m[1]);
        const end = m[2] ? Number(m[2]) : hit.bytes.length - 1;
        return { ...base, range: { offset, length: end - offset + 1 }, body: new Response(hit.bytes.slice(offset, end + 1)).body! };
      }
      return { ...base, body: new Response(hit.bytes).body! };
    },
    async put(key, value, options) {
      const bytes = value instanceof ArrayBuffer ? new Uint8Array(value) : new Uint8Array(await new Response(value).arrayBuffer());
      store.set(key, { bytes, meta: options?.httpMetadata });
    },
  };
}

function deps(calls: string[]): DriveDeps {
  return {
    now: () => now,
    async fetch(input, init) {
      const url = new URL(String(input));
      calls.push(`${url.pathname}${url.search}`);
      if (url.pathname === "/cdn-cgi/access/certs") return Response.json({ keys: [accessJwk] });
      if (url.hostname === "oauth2.googleapis.com") {
        const assertion = new URLSearchParams(String(init?.body)).get("assertion")!;
        const [h, p, s] = assertion.split(".");
        const ok = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", saPair.publicKey, Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/") + "==".slice(0, (4 - (s.length % 4)) % 4)), (c) => c.charCodeAt(0)), enc.encode(`${h}.${p}`));
        return ok ? Response.json({ access_token: "ya29.test", expires_in: 3600 }) : new Response("bad", { status: 400 });
      }
      expect(new Headers(init?.headers).get("authorization")).toBe("Bearer ya29.test");
      const exportMatch = /^\/drive\/v3\/files\/([^/]+)\/export$/.exec(url.pathname);
      if (exportMatch) return new Response(`# ${FILES[exportMatch[1]].name}`, { headers: { "content-type": url.searchParams.get("mimeType")! } });
      const fileMatch = /^\/drive\/v3\/files\/([^/]+)$/.exec(url.pathname);
      if (fileMatch) {
        const f = FILES[decodeURIComponent(fileMatch[1])];
        if (!f) return new Response("nf", { status: 404 });
        if (url.searchParams.get("alt") === "media") {
          const range = new Headers(init?.headers).get("range");
          if (range) return new Response(f.bytes!.slice(2, 5), { status: 206, headers: { "content-range": `bytes 2-4/${f.size}`, "content-length": "3" } });
          return new Response(f.bytes!, { headers: { "content-length": String(f.size) } });
        }
        const { bytes: _b, ...meta } = f;
        return Response.json(meta);
      }
      if (url.pathname === "/drive/v3/files") {
        const q = url.searchParams.get("q")!;
        const parent = /^'([^']+)' in parents/.exec(q)![1];
        const name = /name='((?:[^'\\]|\\.)*)'/.exec(q)?.[1];
        const files = Object.values(FILES)
          .filter((f) => f.parents?.includes(parent) && (name === undefined || f.name === name))
          .map(({ bytes: _b, ...meta }) => meta);
        return Response.json({ files });
      }
      return new Response("unexpected", { status: 500 });
    },
  };
}

const env = (bucket = memoryBucket()): DriveEnv & { DRIVE_CACHE: ReturnType<typeof memoryBucket> } => ({
  DRIVE_CACHE: bucket,
  GOOGLE_SA_KEY: SA_KEY,
  DRIVE_ROOT_ID: ROOT,
  ACCESS_TEAM_DOMAIN: TEAM,
  ACCESS_AUD: AUD,
});

const get = async (path: string, headers: Record<string, string> = {}, claims: Record<string, unknown> = userClaims) =>
  new Request(`https://fw.aphrody.com${path}`, { headers: { "cf-access-jwt-assertion": await jwt(claims), ...headers } });

beforeEach(() => resetDriveCaches());

describe("drive CDN: Cloudflare Access", () => {
  test("fails closed when Access is not configured", async () => {
    const res = await handleDrive(new Request("https://fw.aphrody.com/drive/"), { DRIVE_ROOT_ID: ROOT }, undefined, deps([]));
    expect(res.status).toBe(503);
  });

  test("rejects missing, foreign-key, wrong-audience and expired tokens", async () => {
    const d = deps([]);
    expect((await handleDrive(new Request("https://fw.aphrody.com/drive/"), env(), undefined, d)).status).toBe(401);
    const forged = new Request("https://fw.aphrody.com/drive/", { headers: { "cf-access-jwt-assertion": await jwt(userClaims, otherPair.privateKey) } });
    expect((await handleDrive(forged, env(), undefined, d)).status).toBe(401);
    expect((await handleDrive(await get("/drive/", {}, { ...userClaims, aud: ["other"] }), env(), undefined, d)).status).toBe(403);
    expect((await handleDrive(await get("/drive/", {}, { ...userClaims, exp: now / 1000 - 1 }), env(), undefined, d)).status).toBe(401);
  });

  test("people come in with an email, agents with a service-token client id", async () => {
    const user = await (await handleDrive(await get("/drive/_whoami"), env(), undefined, deps([]))).json();
    expect(user.viewer).toEqual({ kind: "user", id: "person@example.com" });
    const svc = await handleDrive(await get("/drive/_whoami", {}, { iss: TEAM, aud: AUD, exp: now / 1000 + 60, common_name: "abc.access" }), env(), undefined, deps([]));
    expect((await svc.json()).viewer).toEqual({ kind: "service", id: "abc.access" });
  });

  test("the CF_Authorization cookie works for browsers", async () => {
    const req = new Request("https://fw.aphrody.com/drive/_whoami", { headers: { cookie: `a=b; CF_Authorization=${await jwt(userClaims)}` } });
    expect((await handleDrive(req, env(), undefined, deps([]))).status).toBe(200);
  });
});

describe("drive CDN: files and folders", () => {
  test("root lists as JSON for agents and as HTML for browsers", async () => {
    const body = await (await handleDrive(await get("/drive/"), env(), undefined, deps([]))).json();
    expect(body.entries.map((e: { name: string; href: string }) => [e.name, e.href])).toEqual([
      ["art", "/drive/art/"],
      ["Notes", "/drive/Notes"],
    ]);
    const html = await handleDrive(await get("/drive/", { accept: "text/html" }), env(), undefined, deps([]));
    expect(html.headers.get("content-type")).toContain("text/html");
    const text = await html.text();
    expect(text).toContain('href="/drive/art/"');
    expect(text).toContain("person@example.com");
  });

  test("a folder path without a trailing slash redirects", async () => {
    const res = await handleDrive(await get("/drive/art"), env(), undefined, deps([]));
    expect(res.status).toBe(308);
    expect(res.headers.get("location")).toBe("https://fw.aphrody.com/drive/art/");
  });

  test("first read fetches Drive and fills R2, second read is a cache hit", async () => {
    const e = env();
    const calls: string[] = [];
    const miss = await handleDrive(await get("/drive/art/logo.svg"), e, undefined, deps(calls));
    expect(miss.status).toBe(200);
    expect(miss.headers.get("x-fw-drive-cache")).toBe("miss");
    expect(await miss.text()).toBe("0123456789");
    expect([...e.DRIVE_CACHE.store.keys()]).toEqual(["drive/img/md5img"]);
    expect(calls.filter((c) => c.includes("alt=media"))).toHaveLength(1);

    const hit = await handleDrive(await get("/drive/art/logo.svg"), e, undefined, deps(calls));
    expect(hit.headers.get("x-fw-drive-cache")).toBe("hit");
    expect(await hit.text()).toBe("0123456789");
    expect(calls.filter((c) => c.includes("alt=media"))).toHaveLength(1);
    expect(hit.headers.get("content-security-policy")).toStartWith("sandbox");
    expect(hit.headers.get("content-type")).toBe("image/svg+xml");
  });

  test("on workerd a miss streams to the caller and into R2 at once", async () => {
    const e = env();
    const pending: Promise<unknown>[] = [];
    const lengths: number[] = [];
    const d = { ...deps([]), fixedLengthStream: (n: number) => (lengths.push(n), new TransformStream()) };
    const res = await handleDrive(await get("/drive/art/logo.svg"), e, { waitUntil: (p) => pending.push(p) }, d);
    expect(res.headers.get("content-length")).toBe("10");
    expect(await res.text()).toBe("0123456789");
    await Promise.all(pending);
    expect(lengths).toEqual([10]);
    expect(new TextDecoder().decode(e.DRIVE_CACHE.store.get("drive/img/md5img")!.bytes)).toBe("0123456789");
  });

  test("cache hits honour Range and If-None-Match", async () => {
    const e = env();
    await (await handleDrive(await get("/drive/art/logo.svg"), e, undefined, deps([]))).arrayBuffer();
    const part = await handleDrive(await get("/drive/art/logo.svg", { range: "bytes=2-4" }), e, undefined, deps([]));
    expect(part.status).toBe(206);
    expect(part.headers.get("content-range")).toBe("bytes 2-4/10");
    expect(await part.text()).toBe("234");
    const etag = part.headers.get("etag")!;
    const same = await handleDrive(await get("/drive/art/logo.svg", { "if-none-match": etag }), e, undefined, deps([]));
    expect(same.status).toBe(304);
  });

  test("a Range miss is served by Drive while the full file fills R2 in the background", async () => {
    const e = env();
    const pending: Promise<unknown>[] = [];
    const res = await handleDrive(await get("/drive/art/logo.svg", { range: "bytes=2-4" }), e, { waitUntil: (p) => pending.push(p) }, deps([]));
    expect(res.status).toBe(206);
    expect(await res.text()).toBe("234");
    await Promise.all(pending);
    expect(new TextDecoder().decode(e.DRIVE_CACHE.store.get("drive/img/md5img")!.bytes)).toBe("0123456789");
  });

  test("Google Docs export to Markdown by default, any format on request", async () => {
    const e = env();
    const md = await handleDrive(await get("/drive/Notes"), e, undefined, deps([]));
    expect(md.headers.get("content-type")).toBe("text/markdown");
    expect(md.headers.get("content-disposition")).toContain("Notes.md");
    expect(await md.text()).toBe("# Notes");
    const pdf = await handleDrive(await get("/drive/Notes?export=application/pdf"), e, undefined, deps([]));
    expect(pdf.headers.get("content-type")).toBe("application/pdf");
    expect([...e.DRIVE_CACHE.store.keys()].sort()).toEqual(["drive/doc/2026-11-13T00:00:00Z/application/pdf", "drive/doc/2026-11-13T00:00:00Z/text/markdown"]);
  });

  test("ids resolve only inside the shared root", async () => {
    expect((await handleDrive(await get("/drive/_id/img"), env(), undefined, deps([]))).status).toBe(200);
    expect((await handleDrive(await get("/drive/_id/outside"), env(), undefined, deps([]))).status).toBe(404);
    expect((await handleDrive(await get("/drive/missing.txt"), env(), undefined, deps([]))).status).toBe(404);
  });

  test("writes are refused", async () => {
    const res = await handleDrive(new Request("https://fw.aphrody.com/drive/x", { method: "PUT", body: "x" }), env(), undefined, deps([]));
    expect(res.status).toBe(405);
  });
});

test("fw-edge mounts the drive CDN at /drive", async () => {
  const res = await worker.fetch(new Request("https://fw.aphrody.com/drive/_whoami"), {});
  expect(res.status).toBe(503);
  expect((await res.json()).error).toContain("Access is not configured");
});
