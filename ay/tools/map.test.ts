import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { mapTool } from "./map.ts";

function repoWith(local: string, tool: boolean): string {
  const root = mkdtempSync(join(tmpdir(), "map-"));
  writeFileSync(join(root, "ay.toml"), "[repos]\n");
  writeFileSync(join(root, "ay.local.toml"), local);
  if (tool) {
    mkdirSync(join(root, "std", "tools"), { recursive: true });
    writeFileSync(join(root, "std", "tools", "map.ts"), "");
  }
  return root;
}

describe("map", () => {
  test("resolves the tool from the standard checkout hint", () => {
    const root = repoWith('[repos.standard]\ncheckout = "std"\n', true);
    expect(mapTool(root)).toBe(join(root, "std", "tools", "map.ts"));
  });

  test("is undefined without a hint or without the tool", () => {
    expect(mapTool(repoWith("", false))).toBeUndefined();
    expect(mapTool(repoWith('[repos.standard]\ncheckout = "std"\n', false))).toBeUndefined();
  });
});
