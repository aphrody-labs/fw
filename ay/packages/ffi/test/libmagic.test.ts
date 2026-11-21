import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { MagicCookie, detectMime, detectMimeBuffer, MAGIC_MIME } from "../src/libmagic.ts";

const workDir = mkdtempSync(join(tmpdir(), "ay-libmagic-"));
afterAll(() => rmSync(workDir, { recursive: true, force: true }));

describe("libmagic (bun:ffi -> libmagic.so.1)", () => {
  test("detect JPEG from buffer magic bytes", () => {
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46]);
    const mime = detectMimeBuffer(jpeg);
    expect(mime).toMatch(/image\/jpeg/);
  });

  test("detect PNG from buffer", () => {
    const png = new Uint8Array([
      0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52,
    ]);
    expect(detectMimeBuffer(png)).toMatch(/image\/png/);
  });

  test("detect GZIP", () => {
    const gz = Bun.gzipSync(new TextEncoder().encode("hello"));
    expect(detectMimeBuffer(gz)).toMatch(/gzip/);
  });

  test("detect plain text", () => {
    const txt = new TextEncoder().encode("Hello, world!\n");
    expect(detectMimeBuffer(txt)).toMatch(/text\/plain/);
  });

  test("detect JSON-like as text", () => {
    const json = new TextEncoder().encode('{"foo":"bar"}');
    expect(detectMimeBuffer(json)).toMatch(/text|json/);
  });

  test("empty buffer -> application/x-empty", () => {
    expect(detectMimeBuffer(new Uint8Array(0))).toBe("application/x-empty");
  });

  test("detect file on disk (a text file as text/plain)", () => {
    const path = join(workDir, "hostname.txt");
    writeFileSync(path, "my-host\n");
    const mime = detectMime(path);
    expect(mime).toMatch(/text\/plain/);
  });

  test("MIME + encoding via MAGIC_MIME flag", () => {
    const c = new MagicCookie(MAGIC_MIME);
    const mime = c.buffer(new TextEncoder().encode("hello world"));
    // format: "text/plain; charset=us-ascii"
    expect(mime).toMatch(/text\/plain.*charset/);
    c.close();
  });

  test("cookie reuse: 1000 detections in reasonable time", () => {
    const c = new MagicCookie();
    const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    const start = Bun.nanoseconds();
    for (let i = 0; i < 1000; i++) c.buffer(png);
    const elapsed = (Bun.nanoseconds() - start) / 1e6;
    c.close();
    expect(elapsed).toBeLessThan(1000);
    console.log(`\n  perf: 1000 detections in ${elapsed.toFixed(1)}ms (cookie reuse)`);
  });

  test("closed cookie throws on use", () => {
    const c = new MagicCookie();
    c.close();
    expect(() => c.buffer(new Uint8Array([0]))).toThrow();
  });

  test("close() is idempotent", () => {
    const c = new MagicCookie();
    c.close();
    expect(() => c.close()).not.toThrow();
  });
});
