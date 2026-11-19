import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
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

  test("<command> --help prints the registry row and the script header without running the tool", async () => {
    const run = (...args: string[]) =>
      Bun.spawnSync([process.execPath, resolve(REPO_ROOT, "apps/ay/src/cli.ts"), ...args], { cwd: REPO_ROOT });
    // shells installs the machine configuration and edge-config rewrites wrangler.jsonc when run, so --help must never reach them.
    for (const name of ["shells", "edge-config", "release-publish"]) {
      const out = run(name, "--help");
      expect(out.exitCode).toBe(0);
      const text = out.stdout.toString();
      expect(text).toContain(`ay ${name}`);
      expect(text).toContain(`script: tools/${name}.ts`);
    }
  });

  test("the compiled single-file binary dispatches without Bun in-process", () => {
    const dir = mkdtempSync(join(tmpdir(), "ay-compile-"));
    try {
      const exe = join(dir, process.platform === "win32" ? "ay.exe" : "ay");
      const build = Bun.spawnSync([process.execPath, "build", "--compile", "--minify", "--bytecode", resolve(REPO_ROOT, "apps/ay/src/cli.ts"), "--outfile", exe]);
      expect(build.exitCode).toBe(0);
      const opts = { cwd: REPO_ROOT };
      expect(Bun.spawnSync([exe, "version"], opts).stdout.toString()).toContain("ay 0.1.0");
      expect(Bun.spawnSync([exe, "hosts", "--help"], opts).stdout.toString()).toContain("script: tools/hosts.ts");
      // a tool runs through the `bun` on PATH from the checkout found above the working directory
      const doctor = Bun.spawnSync([exe, "doctor"], opts);
      expect(doctor.stdout.toString()).toContain("status: ok");
    } finally {
      rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    }
  }, 60_000);
});
