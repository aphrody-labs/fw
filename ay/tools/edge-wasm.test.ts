import { expect, test } from "bun:test";
import { describePackage, osOf, renderRegistry } from "./edge-wasm.ts";
import { parseRange } from "../apps/edge/src/services/wasm.ts";

// (module (func (export "id") (param i32) (result i32) local.get 0))
const ID = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 6, 1, 96, 1, 127, 1, 127, 3, 2, 1, 0, 7, 6, 1, 2, 105, 100, 0, 0, 10, 6, 1, 4, 0, 32, 0, 11]);

test("a package carries digest, size, imports, exports and the bundling decision", () => {
  const p = describePackage("x", "ay", "base", ID, 1024);
  expect(p).toMatchObject({ name: "x", arch: "wasm32", os: "any", size: ID.byteLength, bundled: true, imports: [], exports: [{ name: "id", kind: "function" }] });
  expect(p.sha256).toMatch(/^[0-9a-f]{64}$/);
  expect(p.r2Key).toBe(`wasm/${p.sha256}.wasm`);
  expect(describePackage("x", "ay", "base", ID, 10).bundled).toBe(false);
});

test("invalid bytes are refused", () => {
  expect(() => describePackage("x", "ay", "base", new Uint8Array([1, 2, 3]), 10)).toThrow();
});

test("os comes from the imports", () => {
  expect(osOf([{ module: "wasi_snapshot_preview1", name: "fd_write", kind: "function" }])).toBe("wasi");
  expect(osOf([{ module: "wbg", name: "x", kind: "function" }])).toBe("browser");
  expect(osOf([{ module: "ay_host", name: "console_put", kind: "function" }])).toBe("any");
});

test("the registry imports bundled modules only", () => {
  const a = describePackage("a", "ay", "base", ID, 1024);
  const b = describePackage("b", "ay", "base", ID, 1);
  const src = renderRegistry([a, b]);
  expect(src).toContain(`"a": { module: module0, bytes: bytes0 }`);
  expect(src).not.toContain("b.wasm");
});

test("ranges", () => {
  expect(parseRange(null, 10)).toBeUndefined();
  expect(parseRange("bytes=2-4", 10)).toEqual({ start: 2, end: 4 });
  expect(parseRange("bytes=8-", 10)).toEqual({ start: 8, end: 9 });
  expect(parseRange("bytes=-3", 10)).toEqual({ start: 7, end: 9 });
  expect(parseRange("bytes=20-30", 10)).toBeNull();
  expect(parseRange("junk", 10)).toBeUndefined();
});
