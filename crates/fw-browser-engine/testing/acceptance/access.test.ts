import { expect, test } from "bun:test";
import { verifyAccess } from "./access.ts";
import { ttl } from "./worker.ts";

const enc = (x: unknown) =>
  btoa(typeof x === "string" ? x : JSON.stringify(x))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
const team = "team";
const aud = "aud-1";
const pair = await crypto.subtle.generateKey(
  { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
  true,
  ["sign", "verify"],
);
const jwk = { ...(await crypto.subtle.exportKey("jwk", pair.publicKey)), kid: "k1" };

async function token(claims: object, kid = "k1", key = pair.privateKey) {
  const head = enc({ alg: "RS256", kid });
  const body = enc(claims);
  const sig = new Uint8Array(
    await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(`${head}.${body}`)),
  );
  return `${head}.${body}.${enc(String.fromCharCode(...sig))}`;
}
const valid = { aud: [aud], iss: `https://${team}.cloudflareaccess.com`, exp: 2_000_000_000 };
const now = 1_900_000_000;

test("a token signed by the team's key, for this audience, issued by the team and not expired is accepted", async () => {
  expect(await verifyAccess(await token(valid), [jwk], team, aud, now)).toBe(true);
});

test("everything else is refused: audience, issuer, expiry, not-before, key id, signature, algorithm, garbage", async () => {
  const other = await crypto.subtle.generateKey(
    { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
    true,
    ["sign", "verify"],
  );
  expect(await verifyAccess(await token({ ...valid, aud: ["other"] }), [jwk], team, aud, now)).toBe(false);
  expect(
    await verifyAccess(await token({ ...valid, iss: "https://evil.cloudflareaccess.com" }), [jwk], team, aud, now),
  ).toBe(false);
  expect(await verifyAccess(await token({ ...valid, exp: now - 1 }), [jwk], team, aud, now)).toBe(false);
  expect(await verifyAccess(await token({ ...valid, nbf: now + 60 }), [jwk], team, aud, now)).toBe(false);
  expect(await verifyAccess(await token(valid, "unknown"), [jwk], team, aud, now)).toBe(false);
  expect(await verifyAccess(await token(valid, "k1", other.privateKey), [jwk], team, aud, now)).toBe(false);
  const t = await token(valid);
  expect(await verifyAccess(t.slice(0, -4) + "AAAA", [jwk], team, aud, now)).toBe(false);
  expect(await verifyAccess(`${enc({ alg: "none" })}.${enc(valid)}.`, [jwk], team, aud, now)).toBe(false);
  expect(await verifyAccess("", [jwk], team, aud, now)).toBe(false);
  expect(await verifyAccess("a.b.c", [jwk], team, aud, now)).toBe(false);
  expect(await verifyAccess(t, [], team, aud, now)).toBe(false);
});

test("nothing behind Access is kept by a shared cache", () => {
  expect(ttl("/shots/r/e/f.png")).toBe(0);
  expect(ttl("/api/events")).toBe(0);
  expect(ttl("/api/state")).toBeLessThanOrEqual(5);
});
