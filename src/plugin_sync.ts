import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { scanNtfsJunctions } from "./ntfs.ts";

export function resolvePluginSource(root: string): string {
  return resolve(root, "plugins", "aphrody");
}

export function resolvePluginManifest(root: string): string {
  return join(resolvePluginSource(root), ".claude-plugin", "plugin.json");
}

function listDirs(dir: string): string[] {
  return existsSync(dir)
    ? readdirSync(dir, { withFileTypes: true })
        .filter((e) => e.isDirectory())
        .map((e) => e.name)
        .sort()
    : [];
}

/** Skills this plugin ships that another plugin under `plugins/` ships too; the unified plugin keeps one copy of each. */
export function auditDeduplication(root: string) {
  const src = resolvePluginSource(root);
  const fwSkills = listDirs(join(src, "skills"));
  const pluginsDir = dirname(src);
  const otherSkills = new Set(
    listDirs(pluginsDir)
      .map((name) => join(pluginsDir, name))
      .filter((dir) => dir !== src)
      .flatMap((dir) => listDirs(join(dir, "skills"))),
  );
  const overlapWithAphrody = fwSkills.filter((s) => otherSkills.has(s));
  const ntfsJunctionsFound = scanNtfsJunctions(src);
  return { ok: overlapWithAphrody.length === 0 && ntfsJunctionsFound.length === 0, fwSkillCount: fwSkills.length, fwSkills, overlapWithAphrody, ntfsJunctionsFound };
}

export function pluginStatus(root: string) {
  const src = resolvePluginSource(root);
  const manifestPath = resolvePluginManifest(root);
  let manifest: { name?: string; version?: string } = {};
  if (existsSync(manifestPath)) {
    try {
      manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    } catch {
      manifest = {};
    }
  }
  return {
    ok: Boolean(manifest.name),
    name: manifest.name ?? null,
    version: manifest.version ?? null,
    sourceDir: src,
    manifestPath,
    skills: listDirs(join(src, "skills")),
    dedupe: auditDeduplication(root),
  };
}

/** Installs the plugin source into Claude Code: `claude plugin update aphrody@fw` (the `fw` marketplace is this repository). */
export function syncFwPlugin(root: string, _options: object = {}): { ok: boolean; sourceDir: string; command: string[]; output?: string; error?: string } {
  const sourceDir = resolvePluginSource(root);
  const command = ["claude", "plugin", "update", "aphrody@fw"];
  const claude = Bun.which("claude");
  if (!claude) return { ok: false, sourceDir, command, error: "the claude CLI is not on PATH" };
  const run = Bun.spawnSync([claude, ...command.slice(1)], { stdout: "pipe", stderr: "pipe" });
  const output = (run.stdout.toString() || run.stderr.toString()).trim();
  return run.exitCode === 0 ? { ok: true, sourceDir, command, output } : { ok: false, sourceDir, command, output, error: `exit ${run.exitCode}` };
}
