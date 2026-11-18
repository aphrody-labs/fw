import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { COMMAND_REGISTRY, findCommand, probeFastTools, readUnifiedStatus, resolveRepoRoot } from "../src/registry.ts";

describe("@aphrody/ay-cli unified registry", () => {
  const root = resolveRepoRoot();

  test("every external command entry points to an existing file", async () => {
    for (const cmd of COMMAND_REGISTRY) {
      if (cmd.kind !== "builtin") {
        expect(cmd.entry).toBeDefined();
        const exists = await Bun.file(join(root, cmd.entry!)).exists();
        expect(exists).toBe(true);
      }
    }
  });

  test("aliases resolve deterministically without collisions", () => {
    const seen = new Set<string>();
    for (const cmd of COMMAND_REGISTRY) {
      expect(seen.has(cmd.name)).toBe(false);
      seen.add(cmd.name);
      expect(findCommand(cmd.name)?.name).toBe(cmd.name);
      for (const alias of cmd.aliases) {
        expect(seen.has(alias)).toBe(false);
        seen.add(alias);
        expect(findCommand(alias)?.name).toBe(cmd.name);
      }
    }
  });

  test("fast modern CLI probe detects installed bun + modern binaries", () => {
    const tools = probeFastTools();
    expect(tools.bun).toBeTruthy();
    expect(tools.rg).toBeTruthy();
    expect(tools.eza).toBeTruthy();
    expect(tools.xh).toBeTruthy();
  });

  test("readUnifiedStatus reads ay.toml products and CLI metadata", async () => {
    const status = await readUnifiedStatus(root);
    expect(status.product.cli).toBe("ay");
    expect(status.products).toContain("os");
    expect(status.products).toContain("cloud");
    expect(status.commands).toContain("web");
    expect(status.commands).toContain("ai");
    expect(status.commands).toContain("re");
    expect(status.commands).toContain("wasm");
  });

  test("CLI executable supports --list --json and status subcommand", async () => {
    const cli = join(root, "apps/ay/src/cli.ts");
    const listProc = Bun.spawnSync([process.execPath, cli, "--list", "--json"], { cwd: root });
    expect(listProc.exitCode).toBe(0);
    const list = JSON.parse(listProc.stdout.toString()) as Array<{ name: string }>;
    expect(list.some((x) => x.name === "web")).toBe(true);

    const statusProc = Bun.spawnSync([process.execPath, cli, "status"], { cwd: root });
    expect(statusProc.exitCode).toBe(0);
    const status = JSON.parse(statusProc.stdout.toString()) as { product: { cli: string } };
    expect(status.product.cli).toBe("ay");
  });
});
