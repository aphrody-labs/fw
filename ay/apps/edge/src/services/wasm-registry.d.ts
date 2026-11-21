// Types of target/edge-wasm/registry.ts for a checkout where tools/edge-wasm.ts has not run (typecheck only): tsconfig `paths` resolves
// the generated file first and falls back to this declaration. It has no runtime, so a bundle without the generated registry fails.
export interface Bundled {
  module: WebAssembly.Module;
  bytes: ArrayBuffer;
}
export const registry: Record<string, Bundled>;
export const index: { schema: string; packages: unknown[] };
