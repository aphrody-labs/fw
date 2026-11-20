// Shape of the wasm-bindgen output behind `#cli-wasm-glue` of @aphrody/cli-wasm (crates/infra/aphrody-command-wasm,
// `--target web`). The published package carries the same declaration in src/wasm-glue.d.ts but nothing includes it, so the
// type checker reports the import inside node_modules as unresolved; declaring it here keeps dependency sources clean.
declare module "#cli-wasm-glue" {
  export type InitInput =
    RequestInfo | URL | Response | BufferSource | WebAssembly.Module;
  export type InitOptions = { module_or_path?: InitInput | Promise<InitInput> };
  export type SyncInitOptions = { module: BufferSource | WebAssembly.Module };

  export interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }
  export default function init(
    input?: InitInput | Promise<InitInput> | InitOptions,
  ): Promise<InitOutput>;
  export function initSync(
    input: BufferSource | WebAssembly.Module | SyncInitOptions,
  ): InitOutput;
  export function initPanicHook(): void;
  export function run(args: string[], stdin: string, filesJson: string): string;
  export function version(): string;
  export function abiVersion(): number;
  export function manifest(): string;
  export function callJson(op: string, input: string): string;
  export function callVec(
    op: string,
    params: string,
    a: Uint8Array,
    b: Uint8Array,
  ): Uint8Array;
}
