// An in-memory R2 bucket and Images binding, enough for the Worker's reads, conditional and range requests.
import type { Manifest } from "../../../tools/media-cdn/manifest.ts";

interface Stored {
  bytes: Uint8Array;
  uploaded: Date;
}

export class FakeBucket {
  objects = new Map<string, Stored>();
  reads: string[] = [];

  put(key: string, value: string | ArrayBuffer | Uint8Array) {
    const bytes = typeof value === "string" ? new TextEncoder().encode(value) : new Uint8Array(value);
    this.objects.set(key, { bytes, uploaded: new Date("2026-11-21T00:00:00Z") });
    return Promise.resolve();
  }

  async get(key: string, options?: { onlyIf?: Headers; range?: Headers }) {
    this.reads.push(key);
    const stored = this.objects.get(key);
    if (!stored) return null;
    const httpEtag = `"${key.length}-${stored.bytes.byteLength}"`;
    const base = { key, size: stored.bytes.byteLength, httpEtag, uploaded: stored.uploaded };
    if (options?.onlyIf?.get("If-None-Match") === httpEtag) return base;
    let bytes = stored.bytes;
    let range: { offset: number; length: number } | undefined;
    const header = options?.range?.get("Range");
    const m = header && /^bytes=(\d*)-(\d*)$/.exec(header);
    if (m) {
      const offset = Number(m[1] || 0);
      const end = m[2] ? Number(m[2]) : stored.bytes.byteLength - 1;
      bytes = stored.bytes.slice(offset, end + 1);
      range = { offset, length: bytes.byteLength };
    }
    return { ...base, range, body: new Blob([bytes]).stream(), json: async () => JSON.parse(new TextDecoder().decode(stored.bytes)), arrayBuffer: async () => bytes.buffer };
  }
}

export class FakeImages {
  calls: { transform: unknown; output: unknown }[] = [];
  input(_stream: ReadableStream) {
    const call = { transform: undefined as unknown, output: undefined as unknown };
    this.calls.push(call);
    const chain = {
      transform: (t: unknown) => ((call.transform = t), chain),
      output: async (o: unknown) => {
        call.output = o;
        return { response: () => new Response("transformed:" + JSON.stringify(call.transform)) };
      },
    };
    return chain;
  }
}

export const manifest: Manifest = {
  schema: 1,
  revision: "0badc0de",
  images: {
    "test-photo": { sha: "aaaa1111", source: "png", width: 4096, height: 2304, alpha: false, fallback: "jpg", widths: [320, 600, 1024, 4096], squares: [48, 144], bytes: {} },
    "test-icon": { sha: "bbbb2222", source: "png", width: 512, height: 512, alpha: true, fallback: "png", widths: [320, 512], squares: [48, 512], bytes: {} },
  },
  video: {
    "test-wide": {
      sha: "cccc3333",
      duration: 4,
      fps: 30,
      poster: "test-wide-poster",
      files: (["av1", "vp9", "h264"] as const).flatMap((codec) =>
        [600, 1024, 1440, 1920].flatMap((width) => [1, 2].map((density) => ({ width, density, codec, pixels: { width: width * density, height: Math.round((width * density * 9) / 16 / 2) * 2 }, bytes: 10, codecs: codec === "av1" ? "av01.0.08M.08" : codec === "vp9" ? "vp09.00.40.08" : "avc1.640028", bitrate: 1000 }))),
      ),
    },
  },
  sprites: { ui: { sha: "dddd4444", symbols: ["home", "search"], bytes: 60 } },
};

export function fakeEnv(withImages = true) {
  const bucket = new FakeBucket();
  const images = new FakeImages();
  void bucket.put("v1/manifest.json", JSON.stringify(manifest));
  for (const [name, e] of Object.entries(manifest.images)) {
    void bucket.put(`v1/src/img/${name}.${e.sha}.${e.source}`, "source-bytes");
    for (const w of e.widths) for (const f of ["avif", "webp", e.fallback]) void bucket.put(`v1/img/${name}.${e.sha}/w${w}.${f}`, `${name} w${w} ${f}`);
    for (const s of e.squares) for (const f of ["avif", "webp", e.fallback]) void bucket.put(`v1/img/${name}.${e.sha}/s${s}.${f}`, `${name} s${s} ${f}`);
  }
  void bucket.put("v1/video/test-wide.cccc3333/1920w-2x-av1.mp4", "0123456789abcdef");
  void bucket.put("v1/sprite/ui.dddd4444.svg", '<svg xmlns="http://www.w3.org/2000/svg"><symbol id="home" viewBox="0 0 24 24"/></svg>');
  const env = { MEDIA: bucket, IMAGES: withImages ? images : undefined } as unknown as Env;
  return { env, bucket, images };
}

export const context = () => {
  const pending: Promise<unknown>[] = [];
  return { ctx: { waitUntil: (p: Promise<unknown>) => void pending.push(p), passThroughOnException() {} } as unknown as ExecutionContext, settle: () => Promise.all(pending) };
};
