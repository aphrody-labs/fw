import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { buildHost, findClang } from "../../tools/corelib.ts";
import { root } from "./build.ts";

const golden = readFileSync(join(root, "corelib/tests/golden/boot.txt"), "utf8");

function transcript(exe: string): string {
  const p = Bun.spawnSync([exe], { stdout: "pipe", stderr: "pipe" });
  expect(p.exitCode).toBe(0);
  return p.stdout.toString();
}

describe("the corelib as a native process", () => {
  test("clang builds it against the platform C library and it prints the shared boot transcript", () => {
    findClang();
    expect(transcript(buildHost("clang"))).toBe(golden);
  });

  // The MSVC ABI path: clang-cl + lld-link, the same sources, the same output.
  test.skipIf(process.platform !== "win32")("clang-cl and lld-link build the same corelib for the MSVC ABI", () => {
    expect(transcript(buildHost("clang-cl"))).toBe(golden);
  });
});
