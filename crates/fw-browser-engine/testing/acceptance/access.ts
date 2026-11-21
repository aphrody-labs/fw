// Cloudflare Access: the Worker serves nobody who has not signed in through Access. The edge adds a signed JWT
// (Cf-Access-Jwt-Assertion); it is checked here against the team's public keys, for its audience, issuer and expiry.
// A secret shared with the origin proves the Worker to the origin; this proves the person to the Worker.

type Jwk = JsonWebKey & { kid?: string };
type Claims = { aud?: string | string[]; iss?: string; exp?: number; nbf?: number };

const b64 = (s: string) => Uint8Array.from(atob(s.replaceAll("-", "+").replaceAll("_", "/")), (c) => c.charCodeAt(0));
const json = (s: string) => JSON.parse(new TextDecoder().decode(b64(s)));

/** True only for a token signed by one of `keys`, issued by the team, for `aud`, inside its validity. */
export async function verifyAccess(
  token: string,
  keys: Jwk[],
  team: string,
  aud: string,
  now = Date.now() / 1000,
): Promise<boolean> {
  try {
    const [head, body, signature] = token.split(".");
    if (!head || !body || !signature) return false;
    const header = json(head) as { alg?: string; kid?: string };
    if (header.alg !== "RS256") return false;
    const jwk = keys.find((k) => k.kid === header.kid);
    if (!jwk) return false;
    const key = await crypto.subtle.importKey("jwk", jwk, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, [
      "verify",
    ]);
    const signed = new TextEncoder().encode(`${head}.${body}`);
    if (!(await crypto.subtle.verify("RSASSA-PKCS1-v1_5", key, b64(signature), signed))) return false;
    const claims = json(body) as Claims;
    const audiences = [claims.aud ?? []].flat();
    return (
      audiences.includes(aud) &&
      claims.iss === `https://${team}.cloudflareaccess.com` &&
      typeof claims.exp === "number" &&
      claims.exp > now &&
      (claims.nbf === undefined || claims.nbf <= now)
    );
  } catch {
    return false; // a token that cannot be read is not a token
  }
}

export async function accessKeys(team: string): Promise<Jwk[]> {
  const res = await fetch(`https://${team}.cloudflareaccess.com/cdn-cgi/access/certs`, {
    cf: { cacheTtl: 3600, cacheEverything: true },
  } as RequestInit);
  if (!res.ok) throw new Error(`Access keys: ${res.status}`);
  return ((await res.json()) as { keys: Jwk[] }).keys;
}
