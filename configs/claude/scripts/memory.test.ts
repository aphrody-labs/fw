import { expect, test } from "bun:test";
import { parseArgs } from "./memory.ts";

test("parseArgs separates words from valued and boolean flags", () => {
  const { cmd, words, flags } = parseArgs([
    "search",
    "never",
    "stash",
    "--kind",
    "memory",
    "--limit",
    "3",
    "--json",
  ]);
  expect(cmd).toBe("search");
  expect(words).toEqual(["never", "stash"]);
  expect(flags.get("--kind")).toBe("memory");
  expect(flags.get("--limit")).toBe("3");
  expect(flags.get("--json")).toBe(true);
});

test("search finds the shipped instructions", async () => {
  const p = Bun.spawn(
    [
      "bun",
      "--no-install",
      "--no-env-file",
      `${import.meta.dir}/memory.ts`,
      "search",
      "zero",
      "deny",
      "lists",
      "--json",
      "--kind",
      "instructions",
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  const hits = JSON.parse(await new Response(p.stdout).text()) as Array<{
    kind: string;
  }>;
  expect(await p.exited).toBe(0);
  expect(hits.length).toBeGreaterThan(0);
  expect(hits.every((h) => h.kind === "instructions")).toBe(true);
});
