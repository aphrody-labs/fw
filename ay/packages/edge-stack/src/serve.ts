// TLS and HTTP/3 for the Bun servers of this repository, read from the environment so no certificate path or port is written in code.
//   AY_TLS_CERT, AY_TLS_KEY   PEM files; without both the server stays plain HTTP/1.1 on loopback or behind a terminator
//   AY_TLS_CA                 optional PEM chain
//   AY_HTTP3                  "1" (default when TLS is set): Bun.serve listens on the same port over TCP (HTTP/1.1, Alt-Svc advertises h3) and UDP (HTTP/3);
//                             "only": UDP only (http1: false); "0": TLS without HTTP/3
// Bun.serve implements HTTP/3 itself (oven-sh/bun docs/runtime/http/server.mdx, experimental). Behind Cloudflare the edge already terminates h3,
// so the origin stays plain; use this where the Bun server is the public listener (docs/edge.md, "HTTP/3").

export interface ListenOptions {
  tls?: { cert: Bun.BunFile; key: Bun.BunFile; ca?: Bun.BunFile };
  http3?: true;
  http1?: false;
}

export type Env = Record<string, string | undefined>;

export function listenOptions(env: Env = Bun.env): ListenOptions {
  const cert = env.AY_TLS_CERT;
  const key = env.AY_TLS_KEY;
  const mode = env.AY_HTTP3 ?? "1";
  if (!cert && !key) {
    if (mode === "only") throw new Error("AY_HTTP3=only needs AY_TLS_CERT and AY_TLS_KEY: HTTP/3 requires TLS");
    return {};
  }
  if (!cert || !key) throw new Error("AY_TLS_CERT and AY_TLS_KEY must be set together");
  if (!["0", "1", "only"].includes(mode)) throw new Error(`AY_HTTP3 must be 0, 1 or only, got ${mode}`);
  const tls = { cert: Bun.file(cert), key: Bun.file(key), ...(env.AY_TLS_CA ? { ca: Bun.file(env.AY_TLS_CA) } : {}) };
  if (mode === "0") return { tls };
  return { tls, http3: true, ...(mode === "only" ? { http1: false as const } : {}) };
}
