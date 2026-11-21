// Runs the hand-written examples of corelib/wat/examples (chapters 05 and 06 of the guide).
// Every example is validated with wasm-tools and executed on Bun (JavaScriptCore). A feature the engine
// rejects is recorded by its exact error; wasmtime, when on PATH (or AY_WASMTIME), runs the same files.
import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { assembleFile, buildCore, root } from "./build.ts";
import { buildWasm } from "../../tools/corelib.ts";

const wasmModule = (bytes: Uint8Array) => new WebAssembly.Module(new Uint8Array(bytes));

const DIR = "corelib/wat/examples";
const path = (name: string) => `${DIR}/${name}.wat`;
const bytes = (name: string) => assembleFile(path(name));
type Exports = Record<string, (...a: any[]) => any> & { memory?: WebAssembly.Memory };

function instantiate(name: string, imports: WebAssembly.Imports = {}): Exports {
  return new WebAssembly.Instance(wasmModule(bytes(name)), imports).exports as Exports;
}

/** The compile error Bun reports for a module, or undefined when Bun accepts it. */
function bunRejects(name: string): string | undefined {
  try {
    wasmModule(bytes(name));
  } catch (e) {
    return String(e);
  }
}

const EXAMPLES = ["simd", "relaxed_simd", "threads", "gc", "exceptions", "tail_call", "multi_memory", "memory64", "custom_page_sizes", "stack_switching", "jspi", "control", "indirect", "traps"];

describe("every example is valid WebAssembly with the proposals it names", () => {
  for (const name of EXAMPLES) {
    test(name, () => {
      const p = Bun.spawnSync(["wasm-tools", "validate", "--features", "all", join(root, path(name))], { stdout: "pipe", stderr: "pipe" });
      expect(p.stderr.toString()).toBe("");
      expect(p.exitCode).toBe(0);
    });
  }
});

describe("the clang-built corelib module (chapters 05 and 06)", () => {
  const validate = (path: string, features: string) => Bun.spawnSync(["wasm-tools", "validate", "--features", features, path], { stdout: "pipe", stderr: "pipe" });

  test("the base variant (-mcpu=mvp) is valid Wasm 1.0, with one import and no target_features section", () => {
    const wasm = buildWasm("base");
    expect(validate(wasm, "mvp").exitCode).toBe(0);
    const sections = Bun.spawnSync(["wasm-tools", "objdump", wasm], { stdout: "pipe" }).stdout.toString();
    expect(sections).toContain("custom \"producers\"");
    expect(sections).not.toContain("target_features");
    expect(new Uint8Array(buildCore()).length).toBeGreaterThan(0);
  });

  test("the simd and atomics variants validate with the proposals they use, and not as Wasm 1.0", () => {
    for (const variant of ["simd", "atomics"] as const) {
      const wasm = buildWasm(variant);
      expect(validate(wasm, "all").exitCode).toBe(0);
      expect(validate(wasm, "mvp").exitCode).not.toBe(0);
    }
  });
});

describe("core semantics (chapter 05)", () => {
  test("br_table dispatch equals the corelib ay_arg_count; loop and br need no goto", () => {
    const e = instantiate("control");
    expect([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 99].map(n => e.arg_count(n))).toEqual([0, 0, 3, 2, 1, 1, 1, 2, 1, 0, 1, 0, 0]);
    expect(e.countdown(5)).toBe(5);
  });

  test("call_indirect checks the callee type at the call", () => {
    const e = instantiate("indirect");
    expect(e.call_nullary(0)).toBe(7);
    expect(() => e.call_nullary(1)).toThrow(/signature that does not match/);
    expect(() => e.call_nullary(2)).toThrow(/Out of bounds call_indirect/);
  });

  test("every trap kind surfaces as a WebAssembly.RuntimeError, and catch_all does not catch it", () => {
    const e = instantiate("traps");
    const trap = (f: () => unknown) => {
      try {
        f();
      } catch (x) {
        expect(x).toBeInstanceOf(WebAssembly.RuntimeError);
        return (x as Error).message;
      }
      throw new Error("no trap");
    };
    expect(trap(() => e.unreachable())).toMatch(/Unreachable/);
    expect(trap(() => e.div(1, 0))).toMatch(/Division by zero/);
    expect(trap(() => e.div(-2147483648, -1))).toMatch(/Integer overflow/);
    expect(trap(() => e.trunc(NaN))).toMatch(/Trunc/);
    expect(trap(() => e.load(65536))).toMatch(/Out of bounds memory access/);
    expect(e.load(65532)).toBe(0);
    expect(trap(() => e.load(65533))).toMatch(/Out of bounds memory access/); // the last byte is in range, the access is not
    expect(trap(() => e.load_offset(1))).toMatch(/Out of bounds memory access/); // base + offset is not wrapped to 32 bits
    expect(trap(() => e.catch_all_does_not_catch_traps())).toMatch(/Unreachable/);
    expect(() => e.recurse()).toThrow(RangeError); // JavaScriptCore reports stack exhaustion as RangeError, wasmtime as a trap
  });
});

describe("on Bun (JavaScriptCore)", () => {
  test("simd: v128 lanes, a vector loop and a lane compare", () => {
    const e = instantiate("simd");
    new Uint32Array(e.memory!.buffer).set([1, 2, 3, 4, 5, 6, 7, 8]);
    expect(e.sum4(0)).toBe(10);
    expect(e.sum(0, 8)).toBe(36);
    expect(e.any_equal(30)).toBe(0b0100);
    expect(e.any_equal(5)).toBe(0);
  });

  test("relaxed simd: fused madd, a clean lane select and the deterministic part of trunc", () => {
    const e = instantiate("relaxed_simd");
    const f32 = new Float32Array(e.memory!.buffer);
    f32.set([2, 3, 4, 5, 10, 10, 10, 10, 1, 1, 1, 1]);
    e.madd(0);
    expect([...f32.slice(12, 16)]).toEqual([21, 31, 41, 51]);
    const u8 = new Uint8Array(e.memory!.buffer);
    u8.fill(0xaa, 0, 16);
    u8.fill(0x55, 16, 32);
    u8.fill(0xff, 32, 48);
    u8.fill(0x00, 32, 40);
    e.laneselect(0);
    expect([...u8.slice(48, 56)]).toEqual(Array(8).fill(0x55));
    expect([...u8.slice(56, 64)]).toEqual(Array(8).fill(0xaa));
    f32.set([1.9, -2.9, 7, 0]);
    e.trunc(0);
    expect([...new Int32Array(e.memory!.buffer, 48, 4)]).toEqual([1, -2, 7, 0]); // in-range lanes only: NaN and overflow differ per CPU
  });

  test("threads: atomic ops, wait timeout, wait mismatch, notify without waiters", () => {
    const memory = new WebAssembly.Memory({ initial: 1, maximum: 1, shared: true });
    const e = instantiate("threads", { env: { memory } });
    expect(e.try_acquire(0)).toBe(1);
    expect(e.try_acquire(0)).toBe(0);
    e.release(0);
    expect(e.try_acquire(0)).toBe(1);
    expect(e.add(8, 5)).toBe(0); // returns the old value
    expect(e.add(8, 5)).toBe(5);
    expect(e.wait(16, 0, 1_000_000n)).toBe(2); // timed out
    expect(e.wait(16, 1, 0n)).toBe(1); // value differs
    expect(e.notify(16, 1)).toBe(0);
    e.fence();
  });

  test("threads: four workers share one memory; lock-guarded and atomic counters stay exact", async () => {
    const memory = new WebAssembly.Memory({ initial: 1, maximum: 1, shared: true });
    const code = `
      self.onmessage = ({ data: { wasm, memory, mode, iters } }) => {
        const x = new WebAssembly.Instance(new WebAssembly.Module(wasm), { env: { memory } }).exports;
        const w = new Int32Array(memory.buffer);
        if (mode === "add") for (let i = 0; i < iters; i++) x.add(8, 1);
        else if (mode === "lock") for (let i = 0; i < iters; i++) { while (!x.try_acquire(0)); w[3] = w[3] + 1; x.release(0); }
        else if (mode === "wait") return postMessage(Number(x.wait(16, 0, 10_000_000_000n)));
        postMessage("done");
      };`;
    const url = URL.createObjectURL(new Blob([code]));
    const wasm = bytes("threads");
    const run = (mode: string, iters: number) =>
      new Promise<unknown>((resolve, reject) => {
        const w = new Worker(url);
        w.onmessage = ev => (resolve(ev.data), w.terminate());
        w.onerror = reject;
        w.postMessage({ wasm, memory, mode, iters });
      });
    const views = new Int32Array(memory.buffer);
    await Promise.all([1, 2, 3, 4].map(() => run("add", 20_000)));
    expect(Atomics.load(views, 2)).toBe(80_000);
    await Promise.all([1, 2, 3, 4].map(() => run("lock", 5_000)));
    expect(Atomics.load(views, 3)).toBe(20_000);
    // wait/notify across threads: notify until a waiter is parked, then the waiter reports "woken" (0).
    const e = new WebAssembly.Instance(wasmModule(wasm), { env: { memory } }).exports as Exports;
    const waiter = run("wait", 0);
    for (let woken = 0; woken === 0; ) woken = e.notify(16, 1);
    expect(await waiter).toBe(0);
  });

  test("gc: structs, subtyping with a checked downcast, arrays, i31", () => {
    const e = instantiate("gc");
    expect(e.area(3, 4)).toBe(12);
    expect(e.sides_of_plain()).toBe(3);
    expect(e.array_sum(5)).toBe(35);
    expect(e.i31_roundtrip(-5)).toBe(-5);
    expect(e.i31_roundtrip(0x7fffffff)).toBe(-1); // only 31 bits survive
    expect(e.is_i31(3)).toBe(1);
    expect(() => e.bad_cast()).toThrow(WebAssembly.RuntimeError);
    expect(() => e.bad_cast()).toThrow(/ref\.cast/);
  });

  test("exceptions: try_table catches the tag payload path; an uncaught throw is a WebAssembly.Exception", () => {
    const e = instantiate("exceptions");
    expect(e.add_or_minus_one(1, 2)).toBe(3);
    expect(e.add_or_minus_one(0x7fffffff, 1)).toBe(-1);
    try {
      e.uncaught();
      throw new Error("unreachable");
    } catch (x) {
      expect(x).toBeInstanceOf(WebAssembly.Exception);
    }
  });

  test("tail calls: ten million tail-recursive steps run in constant stack", () => {
    const e = instantiate("tail_call");
    expect(e.sum_to(10)).toBe(55n);
    expect(e.sum_to(10_000_000)).toBe(50_000_005_000_000n);
    expect(e.is_even(1_000_001)).toBe(0);
    expect(e.is_even(10)).toBe(1);
  });

  test("multiple memories: copy from the app memory into the kernel memory, with per-memory bounds", () => {
    const e = instantiate("multi_memory");
    expect(e.sizes()).toEqual([1, 1]);
    e.copy_in(0, 100, 18);
    expect(new TextDecoder().decode(new Uint8Array((e.kernel as unknown as WebAssembly.Memory).buffer, 100, 18))).toBe("hello from ay app\n");
    expect(e.kernel_byte(100)).toBe(0x68);
    expect(() => e.copy_in(65_535, 0, 4)).toThrow(WebAssembly.RuntimeError); // source range leaves the app memory
    expect(e.grow_app(1)).toBe(1);
    expect(e.sizes()).toEqual([1, 2]); // growing the app memory leaves the kernel memory alone
  });

  test("jspi: a synchronous Wasm loop suspends on an async import", async () => {
    const WA = WebAssembly as unknown as { Suspending: new (f: Function) => Function; promising: (f: Function) => (...a: number[]) => Promise<number> };
    const inst = new WebAssembly.Instance(wasmModule(bytes("jspi")), {
      ay: { fetch_word: new WA.Suspending(async (i: number) => (await Bun.sleep(1), i * 10)) },
    });
    const total = WA.promising(inst.exports.total as Function);
    const p = total(4);
    expect(p).toBeInstanceOf(Promise);
    expect(await p).toBe(60);
    expect(() => (inst.exports.total as Function)(3)).toThrow(/outside of a promising/);
  });

  // The three below are not implemented by this Bun build: the exact error is the record, and the
  // behaviour is asserted the moment a newer build starts accepting the module.
  test("memory64: i64 addresses", () => {
    const err = bunRejects("memory64");
    if (err) return expect(err).toContain("Memory64 is not enabled");
    const e = instantiate("memory64");
    expect(e.size()).toBe(1n);
    expect(e.load8(1n)).toBe(0x65);
    expect(() => e.far()).toThrow(WebAssembly.RuntimeError);
  });

  test("custom page sizes: a one-byte page", () => {
    const err = bunRejects("custom_page_sizes");
    if (err) return expect(err).toContain("resizable limits flag are not valid 08");
    const e = instantiate("custom_page_sizes");
    expect(e.size()).toBe(1024);
    expect(e.byte(0)).toBe(0x61);
  });

  test("stack switching: continuations", () => {
    const err = bunRejects("stack_switching");
    if (err) return expect(err).toContain("non-Func, non-Struct, and non-Array");
    expect(instantiate("stack_switching").yields()).toBe(2);
  });
});

const wasmtime = process.env.AY_WASMTIME ?? Bun.which("wasmtime");

describe.skipIf(!wasmtime)("on wasmtime", () => {
  const cases: [name: string, flags: string[], fn: string, args: string[], out: string][] = [
    ["exceptions", [], "add_or_minus_one", ["2147483647", "1"], "-1"],
    ["tail_call", [], "sum_to", ["10000000"], "50000005000000"],
    ["gc", [], "area", ["3", "4"], "12"],
    ["multi_memory", [], "sizes", [], "1\n1"],
    ["memory64", ["-W", "memory64"], "load8", ["1"], "101"],
    ["custom_page_sizes", ["-W", "custom-page-sizes"], "size", [], "1024"],
  ];
  // wasmtime 50.0.2 reports "the wasm_stack_switching feature is not supported on this compiler configuration" on Windows.
  if (process.platform === "linux") cases.push(["stack_switching", ["-W", "stack-switching,function-references,exceptions,gc"], "yields", [], "2"]);
  for (const [name, flags, fn, args, out] of cases) {
    test(`${name}.${fn}`, () => {
      const p = Bun.spawnSync([wasmtime!, "run", ...flags, "--invoke", fn, join(root, path(name)), ...args], { stdout: "pipe", stderr: "pipe" });
      expect(p.exitCode).toBe(0);
      expect(p.stdout.toString().trim().replaceAll("\r", "")).toBe(out);
    });
  }
});
