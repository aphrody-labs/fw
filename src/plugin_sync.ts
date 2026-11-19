import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

export function resolvePluginSource(root: string): string {
  return resolve(root, "plugin");
}

export function resolveGeminiPluginPaths() {
  const geminiRoot = join(homedir(), ".gemini");
  return {
    geminiRoot,
    trackedDest: join(geminiRoot, "plugins", "fairwind"),
    configDest: join(geminiRoot, "config", "plugins", "fairwind"),
    installScript: join(geminiRoot, "scripts", "install.ts"),
  };
}

function mirrorRealDir(src: string, dst: string): void {
  mkdirSync(dst, { recursive: true });
  const srcNames = new Set(readdirSync(src));
  for (const existing of readdirSync(dst)) {
    if (!srcNames.has(existing)) {
      rmSync(join(dst, existing), { recursive: true, force: true });
    }
  }
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    const from = join(src, entry.name);
    const to = join(dst, entry.name);
    if (entry.isDirectory()) {
      mirrorRealDir(from, to);
    } else {
      cpSync(from, to, { force: true });
    }
  }
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
    mirrorRealDir(src, trackedDest);
    mirrorRealDir(src, configDest);
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
  };
}
