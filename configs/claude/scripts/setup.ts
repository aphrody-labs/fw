import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { HOME, REPO_ROOT, runCmd } from "./lib.ts";
import { syncInstructionFiles } from "./sync.ts";

/** Plugins the config enables: settings.json is the source of truth, nothing is hardcoded here. */
export function enabledPlugins(
  settingsPath = join(REPO_ROOT, "settings.json"),
): Set<string> {
  const settings = JSON.parse(readFileSync(settingsPath, "utf8")) as {
    enabledPlugins?: Record<string, boolean>;
  };
  return new Set(
    Object.entries(settings.enabledPlugins ?? {})
      .filter(([, on]) => on)
      .map(([id]) => id),
  );
}

/** Marketplaces to register: the local one plus every extraKnownMarketplaces source. */
export function knownMarketplaces(
  settingsPath = join(REPO_ROOT, "settings.json"),
): string[] {
  const settings = JSON.parse(readFileSync(settingsPath, "utf8")) as {
    extraKnownMarketplaces?: Record<
      string,
      { source: { source: string; repo?: string; path?: string } }
    >;
  };
  return Object.values(settings.extraKnownMarketplaces ?? {})
    .map((m) => m.source.repo ?? m.source.path ?? "")
    .filter(Boolean);
}

const KEEP_CLAUDE_PLUGINS = enabledPlugins();

function setupGit(): void {
  const gitconfig = resolve(REPO_ROOT, "gitconfig");
  const includes = runCmd([
    "git",
    "config",
    "--global",
    "--get-all",
    "include.path",
  ]).out.split(/\r?\n/);
  if (
    !includes.some(
      (l) => l.trim().replaceAll("\\", "/") === gitconfig.replaceAll("\\", "/"),
    )
  ) {
    runCmd(["git", "config", "--global", "--add", "include.path", gitconfig]);
  }
  for (const key of ["user.name", "user.email"]) {
    runCmd(["git", "config", "--global", "--unset-all", key]);
  }
  if (Bun.which("gh")) {
    runCmd(["gh", "auth", "setup-git"]);
  }
}

async function setupClaudePlugins(): Promise<void> {
  if (!Bun.which("claude")) return;

  const listRes = runCmd(["claude", "plugin", "list", "--json"]);
  if (listRes.ok && listRes.out) {
    try {
      const installed = JSON.parse(listRes.out) as Array<{ id: string }>;
      const toRemove = installed
        .filter((p) => !KEEP_CLAUDE_PLUGINS.has(p.id))
        .map((p) => p.id);
      if (toRemove.length > 0) {
        await Promise.all(
          toRemove.map(async (id) => {
            const p = Bun.spawn(
              ["claude", "plugin", "uninstall", id, "--scope", "user"],
              {
                cwd: REPO_ROOT,
                stdout: "ignore",
                stderr: "ignore",
                stdin: "ignore",
              },
            );
            await p.exited;
          }),
        );
      }
    } catch {
      // Ignore parse failure.
    }
  }

  for (const source of knownMarketplaces()) {
    runCmd(["claude", "plugin", "marketplace", "add", source]);
  }

  for (const id of KEEP_CLAUDE_PLUGINS) {
    const res = runCmd([
      "claude",
      "plugin",
      "install",
      id,
      "--scope",
      "user",
      "-y",
    ]);
    if (!res.ok)
      console.error(`claude plugin install ${id}: ${res.err || res.out}`);
    runCmd(["claude", "plugin", "update", id, "--scope", "user"]);
  }
}

function cleanPiSettings(): void {
  const piSettings = join(HOME, ".pi", "agent", "settings.json");
  if (existsSync(piSettings)) {
    try {
      const data = JSON.parse(readFileSync(piSettings, "utf8")) as Record<
        string,
        unknown
      >;
      if (typeof data.envFile === "string" && !existsSync(data.envFile)) {
        delete data.envFile;
        writeFileSync(piSettings, `${JSON.stringify(data, null, 2)}\n`, "utf8");
      }
    } catch {
      // Ignore invalid JSON.
    }
  }
}

if (import.meta.main) {
  runCmd([
    "bun",
    "--no-install",
    join(REPO_ROOT, "scripts", "merge-brain.ts"),
    "--cleanup",
  ]);
  // Hostinger stdio MCP servers (token from HOSTINGER_API_TOKEN, no OAuth); version pinned, referenced by absolute path in the plugin.
  runCmd(["bun", "add", "-g", "@hostinger/mcp@4.3.0"]);
  setupGit();
  cleanPiSettings();
  await setupClaudePlugins();
  syncInstructionFiles();
  runCmd(["bun", "--no-install", join(REPO_ROOT, "scripts", "merge-brain.ts")]);
  console.log("Setup complete.");
}
