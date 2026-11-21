// WOFF2 encoder: woff2/ (Rust, ttf2woff2 + brotli, wasm with no imports), built by tools/material-cdn/wasm.ts.
import woff2Wasm from "../wasm/woff2.wasm";

interface Woff2Exports {
  memory: WebAssembly.Memory;
  alloc(n: number): number;
  dealloc(p: number, n: number): void;
  encode(p: number, n: number, quality: number, transform: number): number;
  result_len(): number;
}

/** Brotli 11 for small inputs (every icon subset and most text subsets), 9 above 1 MB of SFNT where 11 costs seconds for a few percent. */
export function qualityFor(sfntBytes: number): number {
  return sfntBytes <= 1 << 20 ? 11 : 9;
}

export function toWoff2(sfnt: Uint8Array, quality = qualityFor(sfnt.length)): Uint8Array {
  const x = new WebAssembly.Instance(woff2Wasm, {}).exports as unknown as Woff2Exports;
  const p = x.alloc(sfnt.length);
  new Uint8Array(x.memory.buffer, p, sfnt.length).set(sfnt);
  const r = x.encode(p, sfnt.length, quality, 0);
  if (!r) throw new Error("woff2 encode failed");
  return new Uint8Array(x.memory.buffer, r, x.result_len()).slice();
}
