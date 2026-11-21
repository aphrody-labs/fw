// A directory-backed stand-in for the R2 bucket: the tree built by tools/material-cdn (FW_MATERIAL_OUT, default .cache/material-cdn-out).
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

export const OUT = resolve(process.env.FW_MATERIAL_OUT ?? join(import.meta.dir, "..", "..", "..", ".cache", "material-cdn-out"));
export const built = existsSync(join(OUT, "v1", "meta.json")) && existsSync(join(import.meta.dir, "..", "wasm", "woff2.wasm"));

export function fakeEnv(): { env: Env; written: Map<string, Uint8Array> } {
  const written = new Map<string, Uint8Array>();
  const object = (bytes: Uint8Array) => ({
    size: bytes.length,
    body: new Response(bytes).body,
    httpEtag: '"test"',
    uploaded: new Date(0),
    text: async () => new TextDecoder().decode(bytes),
    json: async () => JSON.parse(new TextDecoder().decode(bytes)),
    arrayBuffer: async () => bytes.slice().buffer,
  });
  const bucket = {
    async get(key: string) {
      const stored = written.get(key);
      if (stored) return object(stored);
      const file = Bun.file(join(OUT, key));
      return (await file.exists()) ? object(new Uint8Array(await file.arrayBuffer())) : null;
    },
    async put(key: string, value: ArrayBuffer | Uint8Array | string) {
      written.set(key, typeof value === "string" ? new TextEncoder().encode(value) : new Uint8Array(value));
    },
  };
  return { env: { MATERIAL: bucket } as unknown as Env, written };
}

export const context = (): ExecutionContext => {
  const pending: Promise<unknown>[] = [];
  return { waitUntil: (p: Promise<unknown>) => void pending.push(p), passThroughOnException() {}, props: {}, get pending() { return Promise.all(pending); } } as unknown as ExecutionContext;
};
