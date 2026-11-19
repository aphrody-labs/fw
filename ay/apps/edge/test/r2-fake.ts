// In-memory R2Bucket for unit tests: get/head with range and conditional headers, put, list. The real binding is exercised by the wrangler dev test.
export interface Stored {
  body: Uint8Array;
  contentType?: string;
}

export class FakeBucket {
  readonly objects = new Map<string, Stored>();

  put(key: string, value: string | Uint8Array, contentType?: string): void {
    this.objects.set(key, { body: typeof value === "string" ? new TextEncoder().encode(value) : value, contentType });
  }

  private view(key: string, s: Stored, range?: { offset: number; length: number }) {
    const etag = `"${key.length}-${s.body.byteLength}"`;
    const slice = range ? s.body.slice(range.offset, range.offset + range.length) : s.body;
    return {
      key,
      size: s.body.byteLength,
      httpEtag: etag,
      range,
      writeHttpMetadata: (h: Headers) => {
        if (s.contentType) h.set("content-type", s.contentType);
      },
      body: new Response(slice).body,
      json: async () => JSON.parse(new TextDecoder().decode(s.body)),
      text: async () => new TextDecoder().decode(s.body),
    };
  }

  async head(key: string) {
    const s = this.objects.get(key);
    if (!s) return null;
    const { body: _b, ...rest } = this.view(key, s);
    return rest;
  }

  async get(key: string, opts?: { range?: Headers; onlyIf?: Headers }) {
    const s = this.objects.get(key);
    if (!s) return null;
    const v = this.view(key, s);
    if (opts?.onlyIf?.get("if-none-match") === v.httpEtag) {
      const { body: _b, ...rest } = v;
      return rest;
    }
    const m = /^bytes=(\d+)-(\d*)$/.exec(opts?.range?.get("range") ?? "");
    if (m) {
      const offset = Number(m[1]);
      const end = m[2] ? Number(m[2]) : s.body.byteLength - 1;
      return this.view(key, s, { offset, length: end - offset + 1 });
    }
    return v;
  }
}

export const bucket = (): FakeBucket & R2Bucket => new FakeBucket() as never;
