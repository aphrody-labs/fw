import { describe, expect, test } from "bun:test";
import { compress, decompress } from "../src/zstd.ts";

describe("zstd (bun:ffi -> libzstd.so.1)", () => {
  const payload = new TextEncoder().encode(
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(100),
  );

  test("round-trip compress -> decompress", () => {
    const compressed = compress(payload, 6);
    expect(compressed.byteLength).toBeLessThan(payload.byteLength);
    const back = decompress(compressed);
    expect(back).toEqual(payload);
  });

  test("compression ratio < 5% on highly redundant text", () => {
    const compressed = compress(payload, 6);
    const ratio = compressed.byteLength / payload.byteLength;
    expect(ratio).toBeLessThan(0.05);
  });

  test("compression level 1 vs 22: higher level = smaller output", () => {
    const low = compress(payload, 1);
    const high = compress(payload, 22);
    expect(high.byteLength).toBeLessThanOrEqual(low.byteLength);
  });

  test("empty input round-trips", () => {
    const empty = new Uint8Array(0);
    const back = decompress(compress(empty));
    expect(back.byteLength).toBe(0);
  });

  test("binary (random) input round-trips", () => {
    const rand = crypto.getRandomValues(new Uint8Array(4096));
    const back = decompress(compress(rand));
    expect(back).toEqual(rand);
  });

  test("decompress rejects invalid input", () => {
    const bogus = new Uint8Array([1, 2, 3, 4, 5]);
    expect(() => decompress(bogus)).toThrow();
  });

  test("1000 round-trips complete in reasonable time", () => {
    const start = Bun.nanoseconds();
    for (let i = 0; i < 1000; i++) {
      decompress(compress(payload, 3));
    }
    const elapsedMs = (Bun.nanoseconds() - start) / 1e6;
    // On a standard VPS ~30-100ms is expected, so a generous ceiling is accepted.
    expect(elapsedMs).toBeLessThan(1000);
    console.log(`\n  perf: 1000 round-trips in ${elapsedMs.toFixed(1)}ms`);
  });

  test("matches Bun.zstdCompressSync output when available", () => {
    if (typeof Bun.zstdCompressSync !== "function") {
      console.log("  skip: Bun.zstdCompressSync not available in this build");
      return;
    }
    const ours = compress(payload, 3);
    const native = Bun.zstdCompressSync(payload, { level: 3 }) as Uint8Array;
    // Sizes may differ slightly depending on internal flags; only check that our output
    // also decompresses through Bun and the other way around.
    expect(new Uint8Array(Bun.zstdDecompressSync!(ours))).toEqual(payload);
    expect(decompress(native)).toEqual(payload);
  });
});
