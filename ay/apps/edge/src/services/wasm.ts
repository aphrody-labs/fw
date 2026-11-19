// WebAssembly packages on the edge. The modules are bundled (tools/edge-wasm.ts generates target/edge-wasm/registry.ts, a static
// import of each one): a Worker cannot compile bytes at runtime, so only a module imported at build time can be instantiated. A package
// too large to bundle is listed with its content-addressed R2 key and served from the releases bucket without being instantiated.
//   GET /index.json                       package index (ay.wasm.v1)
//   GET /<name>.wasm | /<sha256>.wasm     the module: application/wasm, ETag = sha256, Range, immutable
//   GET /invoke/<fn>?pkg=<name>&...       call a pure export of a bundled corelib module and return the result as JSON
import { index, registry, type Bundled } from "../../../../target/edge-wasm/registry.ts";
import type { Routed, Service } from "../types.ts";

export interface PackageInfo {
  name: string;
  product: string;
  variant: string;
  sha256: string;
  size: number;
  bundled: boolean;
  r2Key: string;
  imports: { module: string; name: string; kind: string }[];
  exports: { name: string; kind: string }[];
}
export interface PackageIndex {
  schema: string;
  packages: PackageInfo[];
}
type Exports = Record<string, any>;

const JSON_HEADERS = { "content-type": "application/json; charset=utf-8", "access-control-allow-origin": "*", "x-content-type-options": "nosniff" };
const json = (body: unknown, status = 200, cache = "public, max-age=60"): Response => Response.json(body, { status, headers: { ...JSON_HEADERS, "cache-control": cache } });

/** Single-range request of `bytes=a-b`; undefined when absent or malformed, null when unsatisfiable. */
export function parseRange(header: string | null, size: number): { start: number; end: number } | undefined | null {
  if (!header) return undefined;
  const m = /^bytes=(\d*)-(\d*)$/.exec(header.trim());
  if (!m || (!m[1] && !m[2])) return undefined;
  let start: number;
  let end: number;
  if (!m[1]) {
    start = Math.max(0, size - Number(m[2]));
    end = size - 1;
  } else {
    start = Number(m[1]);
    end = m[2] ? Math.min(Number(m[2]), size - 1) : size - 1;
  }
  return start >= size || end < start ? null : { start, end };
}

/** Instances stay in the isolate: instantiating is cheap but the exported memory is reused across requests. */
const instances = new Map<string, Exports>();
export function instantiate(pkg: PackageInfo, bundled: Bundled): Exports {
  let exports = instances.get(pkg.name);
  if (!exports) {
    const imports: WebAssembly.Imports = {};
    for (const i of pkg.imports) {
      if (i.kind !== "function") throw new Error(`${pkg.name}: import ${i.module}.${i.name} is a ${i.kind}, the edge host provides functions only`);
      (imports[i.module] ??= {})[i.name] = () => 0;
    }
    exports = new WebAssembly.Instance(bundled.module, imports).exports as Exports;
    instances.set(pkg.name, exports);
  }
  return exports;
}

const int = (q: URLSearchParams, key: string): number => {
  const raw = q.get(key);
  if (raw === null || !/^-?\d+$/.test(raw)) throw new RangeError(`${key} must be an integer`);
  return Number(raw);
};
const words = (q: URLSearchParams): number[] => {
  const raw = q.get("words") ?? "";
  const list = raw === "" ? [] : raw.split(",").map(Number);
  if (list.length > 16 || list.some(w => !Number.isInteger(w) || w < 0 || w > 0xffff_ffff)) throw new RangeError("words must be at most 16 comma-separated u32");
  return list;
};

/** The pure exports of the corelib that a request may call: argument decoding and result rendering. */
export const INVOKE: Record<string, (x: Exports, q: URLSearchParams) => unknown> = {
  ay_abi_version: x => x.ay_abi_version(),
  ay_arg_count: (x, q) => x.ay_arg_count(int(q, "nr")),
  ay_rights_allow: (x, q) => x.ay_rights_allow(int(q, "have"), int(q, "need")),
  ay_first_free: (x, q) => x.ay_first_free(BigInt.asUintN(64, BigInt(q.get("bits") ?? "0"))),
  ay_sum: (x, q) => withWords(x, q, (ptr, n) => x.ay_sum(ptr, n)),
  ay_checksum: (x, q) => withWords(x, q, (ptr, n) => x.ay_checksum(ptr, n) >>> 0),
};

function withWords(x: Exports, q: URLSearchParams, call: (ptr: number, n: number) => number): number {
  const list = words(q);
  const ptr: number = x.ay_scratch();
  new Uint32Array((x.memory as WebAssembly.Memory).buffer, ptr, 16).set(list);
  return call(ptr, list.length);
}

export function createWasmService(packages: PackageIndex, bundles: Record<string, Bundled>): Service {
  const byName = new Map(packages.packages.map(p => [p.name, p]));
  const bySha = new Map(packages.packages.map(p => [p.sha256, p]));

  async function bytesOf(pkg: PackageInfo, r: Routed): Promise<{ body: ArrayBuffer | ReadableStream; size: number } | undefined> {
    const local = bundles[pkg.name];
    if (local) return { body: local.bytes, size: local.bytes.byteLength };
    const object = await r.env.RELEASES.get(pkg.r2Key);
    return object ? { body: object.body, size: object.size } : undefined;
  }

  async function module(r: Routed, file: string): Promise<Response> {
    const key = file.slice(0, -".wasm".length);
    const pkg = byName.get(key) ?? bySha.get(key);
    if (!pkg) return json({ error: "unknown module", name: key }, 404, "no-store");
    const etag = `"${pkg.sha256}"`;
    const headers = new Headers({ "content-type": "application/wasm", etag, "accept-ranges": "bytes", "cache-control": "public, max-age=31536000, immutable", "access-control-allow-origin": "*", "x-content-type-options": "nosniff" });
    if (r.request.headers.get("if-none-match") === etag) return new Response(null, { status: 304, headers });
    if (r.request.method === "HEAD") {
      headers.set("content-length", String(pkg.size));
      return new Response(null, { headers });
    }
    const found = await bytesOf(pkg, r);
    if (!found) return json({ error: "module missing from the bucket", key: pkg.r2Key }, 404, "no-store");
    const range = parseRange(r.request.headers.get("range"), found.size);
    if (range === null) return new Response(null, { status: 416, headers: { "content-range": `bytes */${found.size}` } });
    if (range && found.body instanceof ArrayBuffer) {
      headers.set("content-range", `bytes ${range.start}-${range.end}/${found.size}`);
      return new Response(found.body.slice(range.start, range.end + 1), { status: 206, headers });
    }
    return new Response(found.body, { headers });
  }

  function invoke(r: Routed, fn: string): Response {
    const run = INVOKE[fn];
    if (!run) return json({ error: "unknown function", functions: Object.keys(INVOKE) }, 404, "no-store");
    const pkg = byName.get(r.url.searchParams.get("pkg") ?? packages.packages[0]?.name ?? "");
    if (!pkg) return json({ error: "unknown package" }, 404, "no-store");
    const bundled = bundles[pkg.name];
    if (!bundled) return json({ error: "package is not bundled, it cannot be instantiated in a Worker", package: pkg.name }, 422, "no-store");
    if (!pkg.exports.some(e => e.name === fn)) return json({ error: "package does not export this function", package: pkg.name, function: fn }, 422, "no-store");
    try {
      const args = Object.fromEntries(r.url.searchParams);
      delete args.pkg;
      return json({ package: pkg.name, sha256: pkg.sha256, function: fn, args, result: run(instantiate(pkg, bundled), r.url.searchParams) }, 200, "no-store");
    } catch (e) {
      if (e instanceof RangeError || e instanceof SyntaxError) return json({ error: e.message }, 400, "no-store");
      throw e;
    }
  }

  return async r => {
    if (r.request.method !== "GET" && r.request.method !== "HEAD") return json({ error: "method not allowed" }, 405, "no-store");
    const path = r.path.replace(/\/+$/, "") || "/";
    if (path === "/" || path === "/index.json") return json(packages);
    if (path.startsWith("/invoke/")) return invoke(r, path.slice("/invoke/".length));
    const file = /^\/([A-Za-z0-9._-]+\.wasm)$/.exec(path);
    if (file) return module(r, file[1]!);
    return json({ error: "not found" }, 404, "no-store");
  };
}

export default createWasmService(index as PackageIndex, registry);
