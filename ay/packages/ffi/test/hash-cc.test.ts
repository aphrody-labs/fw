import { describe, expect, test } from "bun:test";
import { djb2, fnv1a64, popcountBuf, xorSum } from "../src/hash-cc.ts";

describe("bun:cc: inline C compilation (TinyCC)", () => {
  const empty = new Uint8Array(0);
  const hello = new TextEncoder().encode("hello");

  test("fnv1a64 on empty -> 0xcbf29ce484222325 (offset basis)", () => {
    expect(fnv1a64(empty)).toBe(0xcbf29ce484222325n);
  });

  test("fnv1a64('hello') is deterministic", () => {
    const h1 = fnv1a64(hello);
    const h2 = fnv1a64(hello);
    expect(h1).toBe(h2);
    // Known FNV-1a 64 value for 'hello':
    expect(h1).toBe(0xa430d84680aabd0bn);
  });

  test("djb2('hello') is deterministic and stable", () => {
    const h = djb2(hello);
    // Value computed from the C code: djb2("hello") = 261238937
    expect(h).toBe(261238937);
    expect(djb2(hello)).toBe(h); // deterministic
  });

  test("xor_sum on 'hello' = 'h'^'e'^'l'^'l'^'o' = 0x62 (computed)", () => {
    let expected = 0;
    for (const b of hello) expected ^= b;
    expect(xorSum(hello)).toBe(expected);
  });

  test("popcount on [0xff] = 8", () => {
    expect(popcountBuf(new Uint8Array([0xff]))).toBe(8);
  });

  test("popcount on [0x00] = 0", () => {
    expect(popcountBuf(new Uint8Array([0x00]))).toBe(0);
  });

  test("popcount on [0x55, 0xaa] = 4 + 4 = 8", () => {
    expect(popcountBuf(new Uint8Array([0x55, 0xaa]))).toBe(8);
  });

  test("fnv1a64 produces distinct hashes for similar inputs", () => {
    const a = new TextEncoder().encode("Hello");
    const b = new TextEncoder().encode("hello");
    expect(fnv1a64(a)).not.toBe(fnv1a64(b));
  });

  test("1M hashes in reasonable time (TinyCC compiled native speed)", () => {
    const data = crypto.getRandomValues(new Uint8Array(64));
    const start = Bun.nanoseconds();
    let acc = 0n;
    for (let i = 0; i < 1_000_000; i++) acc ^= fnv1a64(data);
    const elapsed = (Bun.nanoseconds() - start) / 1e6;
    expect(elapsed).toBeLessThan(5000);
    console.log(`\n  perf: 1M fnv1a64(64B) in ${elapsed.toFixed(1)}ms`);
    // anti-DCE
    expect(acc).toBeDefined();
  });

  test("djb2 and xor_sum on empty input return the initial values", () => {
    expect(djb2(empty)).toBe(5381);
    expect(xorSum(empty)).toBe(0);
    expect(popcountBuf(empty)).toBe(0);
  });
});
