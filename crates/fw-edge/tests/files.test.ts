// End-to-end tests of fw-drive on fw-edge (`/files`: D1 metadata, R2 blobs) against `wrangler dev --local`.
// A Bun mock server stands in for Cloudflare Access (it serves the team certs); the tests sign the JWTs.
// Run from crates/fw-edge after `worker-build --release`: `bun test tests/files.test.ts`.
import { afterAll, beforeAll, describe, expect, setDefaultTimeout, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startEdge, type Edge } from "./harness.ts";

setDefaultTimeout(90_000);

const AUD = "aud-files-test";
const enc = new TextEncoder();
const b64url = (b: Uint8Array | string) =>
  btoa(String.fromCharCode(...(typeof b === "string" ? enc.encode(b) : b))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const pair = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
const jwk = { ...(await crypto.subtle.exportKey("jwk", pair.publicKey)), kid: "k1" };

let team = "";
async function jwt(claims: Record<string, unknown>): Promise<string> {
  const unsigned = `${b64url(JSON.stringify({ alg: "RS256", kid: "k1" }))}.${b64url(JSON.stringify({ iss: team, aud: [AUD], exp: Math.floor(Date.now() / 1000) + 600, ...claims }))}`;
  return `${unsigned}.${b64url(new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", pair.privateKey, enc.encode(unsigned))))}`;
}
const person = async (email: string) => ({ "cf-access-jwt-assertion": await jwt({ email }) });
const agent = async (id: string) => ({ "cf-access-jwt-assertion": await jwt({ common_name: id }) });

let mock: ReturnType<typeof Bun.serve>;
let edge: Edge;
let state = "";
let base = "";

beforeAll(async () => {
  mock = Bun.serve({ port: 0, fetch: (req) => (new URL(req.url).pathname === "/cdn-cgi/access/certs" ? Response.json({ keys: [jwk] }) : new Response("not found", { status: 404 })) });
  team = `http://127.0.0.1:${mock.port}`;
  state = await mkdtemp(join(tmpdir(), "fw-edge-files-"));
  edge = await startEdge({ persistTo: state, vars: { ACCESS_TEAM_DOMAIN: team, ACCESS_AUD: AUD } });
  base = edge.url;
});

afterAll(async () => {
  await edge?.stop();
  mock?.stop(true);
  if (state) await rm(state, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 });
});

const api = async (path: string, init: RequestInit & { as?: Record<string, string> } = {}) => {
  const { as, ...rest } = init;
  return fetch(`${base}/files${path}`, { ...rest, headers: { ...(as ?? {}), ...(rest.headers as Record<string, string> | undefined) }, redirect: "manual" });
};

describe("fw-drive on D1 and R2 behind Access", () => {
  test("refuses requests without a valid Access token", async () => {
    const none = await api("/api/v1/me");
    expect(none.status).toBe(401);
    const forged = await api("/api/v1/me", { headers: { "cf-access-authenticated-user-email": "mallory@example.com" } });
    expect(forged.status).toBe(401);
    const wrongAud = await api("/api/v1/me", { as: { "cf-access-jwt-assertion": await jwt({ email: "a@example.com", aud: ["other"] }) } });
    expect(wrongAud.status).toBe(403);
  });

  test("people and agents get their own drive", async () => {
    const me = await (await api("/api/v1/me", { as: await person("Me@Example.com") })).json();
    expect(me.ok).toBe(true);
    expect(me.user).toBe("me@example.com");
    const bot = await (await api("/api/v1/me", { as: await agent("abc123.access") })).json();
    expect(bot.user).toBe("abc123.access@service.access");
    expect(bot.root.id).not.toBe(me.root.id);
  });

  test("upload, list, download with Range and ETag, rename, star, trash, restore, delete", async () => {
    const as = await person("files@example.com");
    const up = await api("/api/v1/files?name=hello.txt", { method: "POST", as, body: "hello world", headers: { "content-type": "text/plain" } });
    expect(up.status).toBe(201);
    const file = (await up.json()).file;
    expect(file.mime).toBe("text/plain");
    expect(file.size).toBe(11);

    const listed = await (await api("/api/v1/files", { as })).json();
    expect(listed.files.map((f: { name: string }) => f.name)).toEqual(["hello.txt"]);

    const full = await api(`/api/v1/files/${file.id}/content`, { as });
    expect(full.status).toBe(200);
    expect(await full.text()).toBe("hello world");
    const etag = full.headers.get("etag")!;
    expect(etag).toBe(`"${file.hash}"`);
    expect(full.headers.get("content-security-policy")).toContain("sandbox");

    const part = await api(`/api/v1/files/${file.id}/content`, { as, headers: { range: "bytes=6-10" } });
    expect(part.status).toBe(206);
    expect(await part.text()).toBe("world");
    expect(part.headers.get("content-range")).toBe("bytes 6-10/11");

    const same = await api(`/api/v1/files/${file.id}/content`, { as, headers: { "if-none-match": etag } });
    expect(same.status).toBe(304);

    const renamed = await api(`/api/v1/files/${file.id}`, { method: "PATCH", as, body: JSON.stringify({ name: "hi.txt", starred: true }), headers: { "content-type": "application/json" } });
    expect((await renamed.json()).file).toMatchObject({ name: "hi.txt", starred: true });
    const starred = await (await api("/api/v1/files?view=starred", { as })).json();
    expect(starred.files).toHaveLength(1);

    await api(`/api/v1/files/${file.id}/trash`, { method: "POST", as });
    expect((await (await api("/api/v1/files", { as })).json()).files).toHaveLength(0);
    expect((await (await api("/api/v1/files?view=trash", { as })).json()).files).toHaveLength(1);
    await api(`/api/v1/files/${file.id}/restore`, { method: "POST", as });
    expect((await (await api("/api/v1/files", { as })).json()).files).toHaveLength(1);

    await api(`/api/v1/files/${file.id}/trash`, { method: "POST", as });
    expect((await (await api(`/api/v1/files/${file.id}`, { method: "DELETE", as })).json()).ok).toBe(true);
    expect((await api(`/api/v1/files/${file.id}/content`, { as })).status).toBe(404);
  });

  test("identical uploads share one blob, deleting one keeps the other readable", async () => {
    const as = await person("dedupe@example.com");
    const a = (await (await api("/api/v1/files?name=a.bin", { method: "POST", as, body: "same bytes" })).json()).file;
    const b = (await (await api("/api/v1/files?name=b.bin", { method: "POST", as, body: "same bytes" })).json()).file;
    expect(a.hash).toBe(b.hash);
    await api(`/api/v1/files/${a.id}/trash`, { method: "POST", as });
    await api(`/api/v1/files/${a.id}`, { method: "DELETE", as });
    expect(await (await api(`/api/v1/files/${b.id}/content`, { as })).text()).toBe("same bytes");
  });

  test("folders, search, sharing between people, and isolation", async () => {
    const owner = await person("owner@example.com");
    const guest = await person("guest@example.com");
    const folder = (await (await api("/api/v1/folders", { method: "POST", as: owner, body: JSON.stringify({ name: "Team" }), headers: { "content-type": "application/json" } })).json()).file;
    const doc = (await (await api(`/api/v1/files?parent=${folder.id}&name=plan.md`, { method: "POST", as: owner, body: "# plan" })).json()).file;
    expect((await api(`/api/v1/files/${doc.id}`, { as: guest })).status).toBe(404);

    const shared = await api(`/api/v1/files/${folder.id}/shares`, { method: "PUT", as: owner, body: JSON.stringify({ principal: "Guest@Example.com", role: "reader" }), headers: { "content-type": "application/json" } });
    expect(shared.status).toBe(200);
    expect((await (await api(`/api/v1/files/${doc.id}`, { as: guest })).json()).role).toBe("reader");
    const denied = await api(`/api/v1/files/${doc.id}`, { method: "PATCH", as: guest, body: JSON.stringify({ name: "x.md" }), headers: { "content-type": "application/json" } });
    expect(denied.status).toBe(403);
    const found = await (await api("/api/v1/files?q=plan", { as: guest })).json();
    expect(found.files.map((f: { name: string }) => f.name)).toEqual(["plan.md"]);
    expect((await (await api("/api/v1/files?view=shared", { as: guest })).json()).files.map((f: { name: string }) => f.name)).toEqual(["Team"]);
  });

  test("quota reports usage; pages render under /files and forms redirect inside it", async () => {
    const as = await person("pages@example.com");
    await api("/api/v1/files?name=q.txt", { method: "POST", as, body: "12345" });
    const quota = await (await api("/api/v1/quota", { as })).json();
    expect(quota.quota.used).toBe(5);

    const home = await api("", { as });
    expect(home.status).toBe(303);
    const folder = home.headers.get("location")!;
    expect(folder.startsWith("/files/folders/")).toBe(true);
    const page = await (await fetch(`${base}${folder}`, { headers: as })).text();
    expect(page).toContain("q.txt");
    expect(page).toContain('href="/files/recent"');
    expect(page).toContain('href="/files/drive.css"');
    const css = await api("/drive.css", { as });
    expect(css.status).toBe(200);
    expect(css.headers.get("content-type")).toContain("text/css");

    const id = folder.split("/").pop()!;
    const form = await fetch(`${base}/files/folders/${id}/new-folder`, { method: "POST", headers: { ...as, "content-type": "application/x-www-form-urlencoded" }, body: "name=Projects", redirect: "manual" });
    expect(form.status).toBe(303);
    expect(form.headers.get("location")).toBe(`/files/folders/${id}`);

    const cross = await fetch(`${base}/files/folders/${id}/new-folder`, { method: "POST", headers: { ...as, origin: "https://evil.example", "content-type": "application/x-www-form-urlencoded" }, body: "name=Evil" });
    expect(cross.status).toBe(403);
  });

  test("an upload over the Worker limit is refused without storing anything", async () => {
    const as = await person("big@example.com");
    const res = await api("/api/v1/files?name=huge.bin", { method: "POST", as, body: new Uint8Array(51 * 1024 * 1024) });
    expect(res.status).toBe(400);
    expect((await res.json()).error).toContain("limited");
    expect((await (await api("/api/v1/files", { as })).json()).files).toHaveLength(0);
  });
});
