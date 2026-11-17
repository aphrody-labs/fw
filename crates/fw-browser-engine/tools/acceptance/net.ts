// What a client sees of a published host: DNS (through DNS-over-HTTPS, so the answer is the public one), the TLS session
// (protocol, cipher, ALPN, certificate chain end) and whether HTTP/3 is advertised (HTTPS record, Alt-Svc) and, when a
// curl built with HTTP/3 exists, actually negotiated. Every part is independent: one failing leaves the others.
import { connect } from "node:tls";

export type NetProbe = {
  host: string;
  at: string;
  dns: {
    a: string[];
    aaaa: string[];
    cname: string[];
    https: { priority: number; target: string; alpn: string[] }[];
    ns: string[];
    error?: string;
  };
  tls: {
    protocol: string | null;
    cipher: string | null;
    alpn: string | null;
    subject: string | null;
    issuer: string | null;
    san: string[];
    valid_from: string | null;
    valid_to: string | null;
    days_left: number | null;
    ms: number;
    error?: string;
  };
  http: {
    status: number | null;
    server: string | null;
    ray: string | null;
    alt_svc: string | null;
    hsts: string | null;
    ms: number;
    error?: string;
  };
  h3: { advertised: boolean; verified: boolean | null; via: string | null; ms: number | null; error?: string };
};

type DohAnswer = { name: string; type: number; data: string };
const doh = async (name: string, type: string): Promise<DohAnswer[]> => {
  const res = await fetch(`https://cloudflare-dns.com/dns-query?name=${encodeURIComponent(name)}&type=${type}`, {
    headers: { accept: "application/dns-json" },
    signal: AbortSignal.timeout(8000),
  });
  return ((await res.json()) as { Answer?: DohAnswer[] }).Answer ?? [];
};

/** SVCB/HTTPS data in presentation form (`1 . alpn="h3,h2" ...`) or in RFC 3597 hex (`\# 25 0001 00 ...`). */
export function parseHttpsRecord(data: string): { priority: number; target: string; alpn: string[] } | null {
  if (data.startsWith("\\#")) {
    const hex = data.split(/\s+/).slice(2).join("");
    const b = Uint8Array.from(hex.match(/../g) ?? [], (h) => parseInt(h, 16));
    if (b.length < 3) return null;
    let at = 2;
    const labels: string[] = [];
    while (b[at]) (labels.push(String.fromCharCode(...b.subarray(at + 1, at + 1 + b[at]))), (at += b[at] + 1));
    at++;
    const alpn: string[] = [];
    while (at + 4 <= b.length) {
      const [key, len] = [(b[at] << 8) | b[at + 1], (b[at + 2] << 8) | b[at + 3]];
      if (key === 1)
        for (let i = at + 4; i < at + 4 + len; i += b[i] + 1)
          alpn.push(String.fromCharCode(...b.subarray(i + 1, i + 1 + b[i])));
      at += 4 + len;
    }
    return { priority: (b[0] << 8) | b[1], target: labels.join(".") || ".", alpn };
  }
  const m = data.match(/^(\d+)\s+(\S+)\s*(.*)$/);
  if (!m) return null;
  const alpn = m[3].match(/alpn="?([^\s"]+)/)?.[1].split(",") ?? [];
  return { priority: Number(m[1]), target: m[2], alpn };
}

async function dns(host: string): Promise<NetProbe["dns"]> {
  const out: NetProbe["dns"] = { a: [], aaaa: [], cname: [], https: [], ns: [] };
  try {
    const [a, aaaa, cname, https, ns] = await Promise.all(
      ["A", "AAAA", "CNAME", "HTTPS", "NS"].map((t) => doh(host, t).catch(() => [])),
    );
    out.a = a.filter((r) => r.type === 1).map((r) => r.data);
    out.aaaa = aaaa.filter((r) => r.type === 28).map((r) => r.data);
    out.cname = [...a, ...aaaa, ...cname].filter((r) => r.type === 5).map((r) => r.data.replace(/\.$/, ""));
    out.https = https.filter((r) => r.type === 65).flatMap((r) => parseHttpsRecord(r.data) ?? []);
    out.ns = ns.filter((r) => r.type === 2).map((r) => r.data.replace(/\.$/, ""));
  } catch (error) {
    out.error = (error as Error).message;
  }
  return out;
}

function tls(host: string): Promise<NetProbe["tls"]> {
  const t0 = performance.now();
  return new Promise((done) => {
    const empty = {
      protocol: null,
      cipher: null,
      alpn: null,
      subject: null,
      issuer: null,
      san: [],
      valid_from: null,
      valid_to: null,
      days_left: null,
    };
    const socket = connect(
      { host, port: 443, servername: host, ALPNProtocols: ["h2", "http/1.1"], timeout: 8000 },
      () => {
        const cert = socket.getPeerCertificate();
        const to = cert.valid_to ? new Date(cert.valid_to) : null;
        done({
          protocol: socket.getProtocol(),
          cipher: socket.getCipher()?.name ?? null,
          alpn: (socket.alpnProtocol as string | false) || null,
          subject: String(cert.subject?.CN ?? "") || null,
          issuer: [cert.issuer?.O, cert.issuer?.CN].filter(Boolean).join(" / ") || null,
          san: (cert.subjectaltname ?? "")
            .split(", ")
            .map((s) => s.replace(/^DNS:/, ""))
            .filter(Boolean),
          valid_from: cert.valid_from ? new Date(cert.valid_from).toISOString() : null,
          valid_to: to?.toISOString() ?? null,
          days_left: to ? Math.floor((to.getTime() - Date.now()) / 86400000) : null,
          ms: Math.round(performance.now() - t0),
        });
        socket.end();
      },
    );
    socket.on("error", (e) => done({ ...empty, ms: Math.round(performance.now() - t0), error: e.message }));
    socket.on(
      "timeout",
      () => (socket.destroy(), done({ ...empty, ms: Math.round(performance.now() - t0), error: "timeout" })),
    );
  });
}

async function http(host: string): Promise<NetProbe["http"]> {
  const t0 = performance.now();
  try {
    const res = await fetch(`https://${host}/`, {
      method: "HEAD",
      redirect: "manual",
      signal: AbortSignal.timeout(10000),
    });
    const h = res.headers;
    return {
      status: res.status,
      server: h.get("server"),
      ray: h.get("cf-ray"),
      alt_svc: h.get("alt-svc"),
      hsts: h.get("strict-transport-security"),
      ms: Math.round(performance.now() - t0),
    };
  } catch (error) {
    return {
      status: null,
      server: null,
      ray: null,
      alt_svc: null,
      hsts: null,
      ms: Math.round(performance.now() - t0),
      error: (error as Error).message,
    };
  }
}

/** Real HTTP/3 needs a client that speaks it: curl built with HTTP3 here, else only the advertisement is judged. */
async function h3(host: string, advertised: boolean): Promise<NetProbe["h3"]> {
  const curl = Bun.which("curl");
  const out: NetProbe["h3"] = { advertised, verified: null, via: null, ms: null };
  if (!curl) return out;
  const features = await new Response(
    Bun.spawn([curl, "--version"], { stdout: "pipe", stderr: "ignore" }).stdout,
  ).text();
  if (!/HTTP3/.test(features)) return out;
  const t0 = performance.now();
  const proc = Bun.spawn(
    [
      curl,
      "-sS",
      "--http3-only",
      "-o",
      "/dev/null",
      "-w",
      "%{http_version} %{http_code}",
      "-m",
      "10",
      `https://${host}/`,
    ],
    {
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const [text, err] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  out.ms = Math.round(performance.now() - t0);
  out.via = "curl --http3-only";
  out.verified = text.startsWith("3 ");
  if (!out.verified) out.error = err.trim().split("\n").at(-1) || `negotiated ${text.trim()}`;
  return out;
}

export async function probeNet(host: string): Promise<NetProbe> {
  const [d, t, h] = await Promise.all([dns(host), tls(host), http(host)]);
  const advertised = d.https.some((r) => r.alpn.includes("h3")) || /\bh3(-\d+)?=/.test(h.alt_svc ?? "");
  return { host, at: new Date().toISOString(), dns: d, tls: t, http: h, h3: await h3(host, advertised) };
}

if (import.meta.main) console.log(JSON.stringify(await probeNet(Bun.argv[2] ?? "cloudflare.com"), null, 1));
