import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, relative, resolve } from "node:path";
import { mirrorRealDirNtfs, scanNtfsJunctions, toNtfsSafePath } from "./ntfs.ts";

export function resolvePluginSource(root: string): string {
  return resolve(root, "plugin");
}

export function resolveGeminiPluginPaths() {
  const geminiRoot = join(homedir(), ".gemini");
  return {
    geminiRoot,
    trackedDest: join(geminiRoot, "plugins", "fairwind"),
    configDest: join(geminiRoot, "config", "plugins", "fairwind"),
    aphrodySkills: join(geminiRoot, "plugins", "aphrody", "skills"),
    installScript: join(geminiRoot, "scripts", "install.ts"),
  };
}

function computeDirLockManifest(dir: string): Record<string, string> {
  const out: Record<string, string> = {};
  const stack = [dir];
  const files: string[] = [];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    for (const entry of readdirSync(toNtfsSafePath(cur), { withFileTypes: true })) {
      const full = join(cur, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile() && entry.name !== "fairwind.plugin.lock.json") files.push(full);
    }
  }
  files.sort();
  for (const f of files) {
    const rel = relative(dir, f).replaceAll("\\", "/");
    const h = new Bun.CryptoHasher("sha256");
    h.update(readFileSync(toNtfsSafePath(f)));
    out[rel] = h.digest("hex");
  }
  return out;
}

export function auditDeduplication(root: string) {
  const src = resolvePluginSource(root);
  const { aphrodySkills, trackedDest, configDest } = resolveGeminiPluginPaths();
  const listSkills = (dir: string) =>
    existsSync(dir)
      ? readdirSync(dir, { withFileTypes: true })
          .filter((e) => e.isDirectory())
          .map((e) => e.name)
          .sort()
      : [];

  const fairwindSkills = listSkills(join(src, "skills"));
  const aphrodySkillList = listSkills(aphrodySkills);
  const aphrodySet = new Set(aphrodySkillList);
  const overlapWithAphrody = fairwindSkills.filter((s) => aphrodySet.has(s));

  const srcLock = existsSync(src) ? computeDirLockManifest(src) : {};
  const trackedLock = existsSync(trackedDest) ? computeDirLockManifest(trackedDest) : {};
  const configLock = existsSync(configDest) ? computeDirLockManifest(configDest) : {};

  const lockJson = JSON.stringify(srcLock);
  const geminiTrackedInSync = lockJson === JSON.stringify(trackedLock);
  const geminiConfigInSync = lockJson === JSON.stringify(configLock);
  const ntfsJunctionsFound = [
    ...scanNtfsJunctions(trackedDest),
    ...scanNtfsJunctions(configDest),
  ];

  return {
    ok: overlapWithAphrody.length === 0 && ntfsJunctionsFound.length === 0,
    fairwindSkillCount: fairwindSkills.length,
    fairwindSkills,
    aphrodySkillCount: aphrodySkillList.length,
    overlapWithAphrody,
    geminiTrackedInSync,
    geminiConfigInSync,
    ntfsJunctionsFound,
  };
}

export function pluginStatus(root: string) {
  const src = resolvePluginSource(root);
  const { trackedDest, configDest } = resolveGeminiPluginPaths();
  let version: string | null = null;
  const manifestPath = join(src, "plugin.json");
  if (existsSync(manifestPath)) {
    try {
      version = (JSON.parse(readFileSync(manifestPath, "utf8")) as { version?: string }).version ?? null;
    } catch {
      version = null;
    }
  }
  const skillsDir = join(src, "skills");
  const skills = existsSync(skillsDir)
    ? readdirSync(skillsDir, { withFileTypes: true })
        .filter((e) => e.isDirectory())
        .map((e) => e.name)
        .sort()
    : [];
  return {
    ok: existsSync(manifestPath),
    name: "fairwind",
    version,
    sourceDir: src,
    skills,
    trackedInstalled: existsSync(join(trackedDest, "plugin.json")),
    configMaterialized: existsSync(join(configDest, "plugin.json")),
    trackedDest,
    configDest,
    dedupe: auditDeduplication(root),
  };
}

export function syncFairwindPlugin(
  root: string,
  options: { runInstall?: boolean; validate?: boolean } = {},
): {
  ok: boolean;
  sourceDir: string;
  trackedDest: string;
  configDest: string;
  validated?: boolean;
  validationOutput?: string;
  lockFileCount?: number;
  error?: string;
} {
  const src = resolvePluginSource(root);
  const { trackedDest, configDest, installScript } = resolveGeminiPluginPaths();
  if (!existsSync(join(src, "plugin.json"))) {
    return {
      ok: false,
      sourceDir: src,
      trackedDest,
      configDest,
      error: `Missing plugin manifest at ${join(src, "plugin.json")}`,
    };
  }

  try {
    mirrorRealDirNtfs(src, trackedDest);
    mirrorRealDirNtfs(src, configDest);
    const files = computeDirLockManifest(src);
    const lockPayload = `${JSON.stringify(
      {
        schema: "fairwind.plugin-lock.v1",
        sourceRepo: "aphrody-labs/fairwind",
        syncedAt: new Date().toISOString(),
        files,
      },
      null,
      2,
    )}\n`;
    writeFileSync(join(trackedDest, "fairwind.plugin.lock.json"), lockPayload, "utf8");
    writeFileSync(join(configDest, "fairwind.plugin.lock.json"), lockPayload, "utf8");
  } catch (err) {
    return {
      ok: false,
      sourceDir: src,
      trackedDest,
      configDest,
      error: err instanceof Error ? err.message : String(err),
    };
  }

  if (options.runInstall && existsSync(installScript)) {
    const inst = Bun.spawnSync(["bun", installScript], { stdout: "pipe", stderr: "pipe" });
    if (inst.exitCode !== 0) {
      return {
        ok: false,
        sourceDir: src,
        trackedDest,
        configDest,
        error: `install.ts failed: ${(inst.stderr.toString() || inst.stdout.toString()).slice(0, 400)}`,
      };
    }
  }

  let validated: boolean | undefined;
  let validationOutput: string | undefined;
  if (options.validate && Bun.which("agy")) {
    const val = Bun.spawnSync(["agy", "plugin", "validate", configDest, "--strict", "--json"], {
      stdout: "pipe",
      stderr: "pipe",
    });
    validated = val.exitCode === 0;
    validationOutput = (val.stdout.toString() || val.stderr.toString()).trim();
    if (!validated) {
      return {
        ok: false,
        sourceDir: src,
        trackedDest,
        configDest,
        validated,
        validationOutput,
        error: `agy plugin validate --strict failed: ${validationOutput.slice(0, 400)}`,
      };
    }
  }

  return {
    ok: true,
    sourceDir: src,
    trackedDest,
    configDest,
    validated,
    validationOutput,
    lockFileCount: Object.keys(computeDirLockManifest(src)).length,
  };
}
