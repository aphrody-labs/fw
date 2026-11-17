import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildSharedLibrary, nativeSource } from "../src/native.ts";

const workDir = mkdtempSync(join(tmpdir(), "ay-native-test-"));
afterAll(() => rmSync(workDir, { recursive: true, force: true }));

describe("native: on-demand shared library build", () => {
  test("builds under os.tmpdir() and reuses the cached library", () => {
    const source = join(workDir, "answer.c");
    writeFileSync(source, "int answer(void) { return 42; }\n");
    const first = buildSharedLibrary({ source, libs: [], owner: "test" });
    expect(first.startsWith(join(tmpdir(), "ay-ffi"))).toBe(true);
    expect(first.endsWith("answer.so")).toBe(true);
    expect(buildSharedLibrary({ source, libs: [], owner: "test" })).toBe(first);
  });

  test("a changed source gets a new cache entry", () => {
    const source = join(workDir, "value.c");
    writeFileSync(source, "int value(void) { return 1; }\n");
    const a = buildSharedLibrary({ source, libs: [], owner: "test" });
    writeFileSync(source, "int value(void) { return 2; }\n");
    const b = buildSharedLibrary({ source, libs: [], owner: "test" });
    expect(b).not.toBe(a);
  });

  test("a compile error reports the owner, the file and the compiler output", () => {
    const source = join(workDir, "broken.c");
    writeFileSync(source, "this is not C\n");
    expect(() => buildSharedLibrary({ source, libs: [], owner: "test-owner" })).toThrow(
      /test-owner: failed to compile broken\.c with gcc/,
    );
  });

  test("nativeSource resolves inside the package native/ directory", () => {
    expect(nativeSource("hash.c").replaceAll("\\", "/").endsWith("/native/hash.c")).toBe(true);
  });
});
