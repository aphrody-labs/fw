import { expect, test } from "bun:test";
import { keyOf } from "./index.ts";

test("a request path maps to a key under ay-tools/ and never leaves it", () => {
  expect(keyOf("/latest.json")).toBe("ay-tools/latest.json");
  expect(keyOf("/160.0.8166.0/windows-x64/siso.exe")).toBe("ay-tools/160.0.8166.0/windows-x64/siso.exe");
  for (const path of ["/", "", "/../secret", "/a/../b", "/a//b", "/./a", "/%2e%2e/x"]) expect(keyOf(path)).toBeUndefined();
});
