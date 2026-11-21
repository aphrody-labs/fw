// Wrangler rules (tools/edge-wasm.ts): *.wasm are compiled by the runtime at upload, *.wasm.bin are raw bytes.
declare module "*.wasm" {
  const module: WebAssembly.Module;
  export default module;
}
declare module "*.wasm.bin" {
  const bytes: ArrayBuffer;
  export default bytes;
}
