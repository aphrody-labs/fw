import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";
import { COMMANDS, resolveCommand } from "../src/registry.ts";
import { formatHelp, generateCompletions } from "../src/cli.ts";

const REPO_ROOT = resolve(import.meta.dir, "../../..");

describe("ay unified CLI", () => {
  test("all non-builtin command targets exist on disk", async () => {
    for (const cmd of COMMANDS) {
      if (cmd.kind === "builtin") continue;
      const file = Bun.file(resolve(REPO_ROOT, cmd.target));
      expect(await file.exists()).toBe(true);
    }
  });

  test("every standalone tools/*.ts script is registered", () => {
    const registeredTargets = new Set(COMMANDS.map((c) => c.target));
    const toolFiles = [...new Bun.Glob("tools/*.ts").scanSync(REPO_ROOT)]
      .map((p) => p.replaceAll("\\", "/"))
      .filter((p) => !p.endsWith(".test.ts"));

    for (const toolFile of toolFiles) {
      expect(registeredTargets.has(toolFile)).toBe(true);
    }
  });

  test("command names and aliases resolve uniquely", () => {
    const seen = new Set<string>();
    for (const cmd of COMMANDS) {
      expect(seen.has(cmd.name)).toBe(false);
      seen.add(cmd.name);
      expect(resolveCommand(cmd.name)?.name).toBe(cmd.name);

      for (const alias of cmd.aliases ?? []) {
        expect(seen.has(alias)).toBe(false);
        seen.add(alias);
        expect(resolveCommand(alias)?.name).toBe(cmd.name);
      }
    }
  });

  test("help and shell completions render cleanly", () => {
    const help = formatHelp();
    expect(help).toContain("ay 0.1.0");
    expect(help).toContain("r2-sync");
    expect(help).toContain("completions");

    for (const shell of ["bash", "zsh", "fish", "nu", "pwsh"]) {
      const out = generateCompletions(shell);
      expect(out.length).toBeGreaterThan(20);
      expect(out).toContain("ay");
    }
  });
});
