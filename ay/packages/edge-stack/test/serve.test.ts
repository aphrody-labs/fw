import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { listenOptions } from "../src/serve.ts";

/** openssl from PATH, else the one that ships with Git for Windows (next to git, or under the Program Files install). */
function openssl(): string | undefined {
  const onPath = Bun.which("openssl");
  if (onPath) return onPath;
  const git = Bun.which("git");
  const candidates = [
    git && join(dirname(git), "openssl.exe"),
    git && join(dirname(dirname(git)), "usr", "bin", "openssl.exe"),
    Bun.env.ProgramFiles && join(Bun.env.ProgramFiles, "Git", "usr", "bin", "openssl.exe"),
  ];
  return candidates.find((c): c is string => !!c && Bun.file(c).size > 0);
}

const dir = mkdtempSync(join(tmpdir(), "ay-h3-"));
afterAll(() => rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));

const ossl = openssl();
const cert = join(dir, "cert.pem");
const key = join(dir, "key.pem");
if (ossl) {
  const gen = Bun.spawnSync([ossl, "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-keyout", key, "-out", cert, "-days", "2", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"], { env: { ...Bun.env, MSYS_NO_PATHCONV: "1" } });
  if (gen.exitCode !== 0) throw new Error(`openssl failed: ${gen.stderr.toString()}`);
}

describe("listenOptions", () => {
  test("no TLS stays plain; HTTP/3 without TLS is refused", () => {
    expect(listenOptions({})).toEqual({});
    expect(() => listenOptions({ AY_HTTP3: "only" })).toThrow("requires TLS");
    expect(() => listenOptions({ AY_TLS_CERT: "c.pem" })).toThrow("together");
  });

  test("TLS enables HTTP/3 by default, AY_HTTP3 selects off or only", () => {
    const env = { AY_TLS_CERT: "c.pem", AY_TLS_KEY: "k.pem" };
    expect(listenOptions(env)).toMatchObject({ http3: true });
    expect(listenOptions(env).http1).toBeUndefined();
    expect(listenOptions({ ...env, AY_HTTP3: "0" }).http3).toBeUndefined();
    expect(listenOptions({ ...env, AY_HTTP3: "only" })).toMatchObject({ http3: true, http1: false });
    expect(() => listenOptions({ ...env, AY_HTTP3: "x" })).toThrow("must be 0, 1 or only");
  });
});

describe.skipIf(!ossl)("HTTP/3 round trip against Bun.serve", () => {
  const env = { AY_TLS_CERT: cert, AY_TLS_KEY: key };
  const get = (port: number, protocol?: "http3") =>
    fetch(`https://127.0.0.1:${port}/ping`, { protocol, tls: { rejectUnauthorized: false } } as RequestInit);

  test("dual stack: HTTP/3 answers and the TCP response advertises it in Alt-Svc", async () => {
    const server = Bun.serve({ port: 0, hostname: "127.0.0.1", ...listenOptions(env), fetch: () => new Response("pong") });
    try {
      const h3 = await get(server.port!, "http3");
      expect(h3.status).toBe(200);
      expect(await h3.text()).toBe("pong");
      const tcp = await get(server.port!);
      expect(tcp.headers.get("alt-svc")).toContain(`h3=":${server.port}"`);
    } finally {
      await server.stop(true);
    }
  });

  test("AY_HTTP3=only: no TCP listener, HTTP/3 still round-trips", async () => {
    const server = Bun.serve({ port: 0, hostname: "127.0.0.1", ...listenOptions({ ...env, AY_HTTP3: "only" }), fetch: () => new Response("pong") });
    try {
      expect(await (await get(server.port!, "http3")).text()).toBe("pong");
      await expect(get(server.port!)).rejects.toThrow();
    } finally {
      await server.stop(true);
    }
  });
});
