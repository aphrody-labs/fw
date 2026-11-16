import { beforeAll, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { assemble, assembleFile, buildCore, root } from "./build.ts";
import { instantiateApp, instantiateCore, run } from "./host.ts";

const golden = readFileSync(join(root, "corelib/tests/golden/boot.txt"), "utf8");
let corelib: Uint8Array;
let hello: Uint8Array;

beforeAll(() => {
  corelib = buildCore();
  hello = assembleFile("corelib/wat/hello.wat");
}, 600_000);

describe("the WebAssembly OS", () => {
  test("corelib.wasm plus the hand-written WAT app print the shared boot transcript", () => {
    expect(run(corelib, [hello])).toBe(golden);
  });

  test("the corelib module imports one host function and the app module imports one syscall", () => {
    const k = WebAssembly.Module.imports(new WebAssembly.Module(corelib));
    expect(k.map(i => `${i.module}.${i.name}`)).toEqual(["ay_host.console_put"]);
    const a = WebAssembly.Module.imports(new WebAssembly.Module(hello));
    expect(a.map(i => `${i.module}.${i.name}`)).toEqual(["ay.syscall"]);
  });

  test("an app cannot name a corelib object it holds no capability for", () => {
    const rogue = assemble(`(module
      (import "ay" "syscall" (func $sys (param i32 i32 i32 i32) (result i32)))
      (global $r (mut i32) (i32.const 0))
      (func (export "step") (result i32)
        ;; LOG through handle 5 (never granted), then LOG through handle 1 (an endpoint, not the console)
        (global.set $r (call $sys (i32.const 2) (i32.const 5) (i32.const 0x41) (i32.const 1)))
        (if (i32.ne (global.get $r) (i32.const -2)) (then (return (i32.const 2))))
        (global.set $r (call $sys (i32.const 2) (i32.const 1) (i32.const 0x41) (i32.const 1)))
        (if (i32.ne (global.get $r) (i32.const -2)) (then (return (i32.const 2))))
        (i32.const 0)))`);
    const out = run(corelib, [rogue]);
    expect(out).toContain("core: task 0 exited\n");
    expect(out).not.toContain("faulted");
    expect(out).not.toContain("A");
  });

  test("the syscall shim forwards typed calls and call_indirect through its table", () => {
    const bytes: number[] = [];
    const k = instantiateCore(corelib, b => bytes.push(b));
    expect(k.ay_spawn()).toBe(0);
    expect(k.ay_sched_next()).toBe(0);
    const shim = instantiateApp(assembleFile("corelib/wat/syscall_shim.wat"), k) as unknown as Record<string, (...a: number[]) => number>;
    expect(shim.version()).toBe(1);
    expect(shim.invoke(0)).toBe(1); // table slot 0 is $version
    expect(shim.invoke(2)).toBe(0); // slot 2 is $frame_alloc: first frame
    expect(shim.invoke(2)).toBe(1);
    expect(() => shim.invoke(3)).toThrow(); // out-of-bounds table access traps
    expect(shim.log(0, 0x0a6b6f, 3)).toBe(3); // "ok\n"
    expect(new TextDecoder().decode(new Uint8Array(bytes))).toBe("ok\n");
  });

  test("the pure ABI functions are callable from the host", () => {
    const k = instantiateCore(corelib, () => {});
    expect(k.ay_abi_version()).toBe(1);
    expect(k.ay_arg_count(2)).toBe(3);
    expect(k.ay_rights_allow(0b101, 0b100)).toBe(1);
    expect(k.ay_rights_allow(0b001, 0b100)).toBe(0);
    expect(k.ay_first_free(0b0111n)).toBe(3);
    expect(k.ay_first_free(0xffff_ffff_ffff_ffffn)).toBe(-1);
    const words = new Uint32Array(k.memory.buffer, k.ay_scratch(), 4);
    words.set([1, 2, 3, 4]);
    expect(k.ay_checksum(k.ay_scratch(), 4) >>> 0).toBe(0xe5bacf84);
    expect(k.ay_sum(k.ay_scratch(), 4)).toBe(10);
    const flag = k.ay_scratch() + 32;
    expect(k.ay_try_acquire(flag)).toBe(1);
    expect(k.ay_try_acquire(flag)).toBe(0);
  });
});
