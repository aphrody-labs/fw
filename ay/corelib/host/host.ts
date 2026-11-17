// The platform driver of the WebAssembly corelib: instantiate the corelib module, give each app module the
// `ay.syscall` import, and loop over the corelib's scheduler. This is the only code outside Wasm.
export type CoreExports = {
  memory: WebAssembly.Memory;
  ay_boot(): number;
  ay_spawn(): number;
  ay_sched_next(): number;
  ay_sched_report(id: number, code: number): void;
  ay_syscall(nr: number, a0: number, a1: number, a2: number): number;
  ay_scratch(): number;
  ay_abi_version(): number;
  ay_rights_allow(have: number, need: number): number;
  ay_arg_count(nr: number): number;
  ay_first_free(bits: bigint): number;
  ay_checksum(ptr: number, len: number): number;
  ay_sum(ptr: number, len: number): number;
  ay_try_acquire(ptr: number): number;
};

export type AppExports = { step(): number };

export function instantiateCore(corelib: Uint8Array, console_put: (byte: number) => void): CoreExports {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array(corelib)), { ay_host: { console_put } });
  return instance.exports as unknown as CoreExports;
}

/** Instantiate an app module: its only way out of its sandbox is the corelib's syscall entry. */
export function instantiateApp(app: Uint8Array, corelib: CoreExports): AppExports & WebAssembly.Exports {
  const imports = { ay: { syscall: corelib.ay_syscall } };
  return new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array(app)), imports).exports as unknown as AppExports;
}

/** Boot the corelib, run every app to completion under its scheduler and return the serial transcript. */
export function run(coreWasm: Uint8Array, appWasms: Uint8Array[]): string {
  const bytes: number[] = [];
  const corelib = instantiateCore(coreWasm, b => bytes.push(b));
  if (corelib.ay_boot() !== 0) throw new Error("corelib self-test failed");
  const apps = new Map<number, AppExports>();
  for (const wasm of appWasms) {
    const id = corelib.ay_spawn();
    if (id < 0) throw new Error(`spawn failed: ${id}`);
    apps.set(id, instantiateApp(wasm, corelib));
  }
  for (let id = corelib.ay_sched_next(); id >= 0; id = corelib.ay_sched_next()) {
    corelib.ay_sched_report(id, apps.get(id)!.step());
  }
  return new TextDecoder().decode(new Uint8Array(bytes));
}
