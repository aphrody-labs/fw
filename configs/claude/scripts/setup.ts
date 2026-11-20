import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  discoverWorkspaces,
  ensureDirLink,
  HOME,
  renderTemplate,
  REPO_ROOT,
  runCmd,
  templateVars,
  writeIfChanged,
} from "./lib.ts";
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

  const mktList = runCmd(["claude", "plugin", "marketplace", "list", "--json"]);
  const hasMkt = mktList.ok && mktList.out.includes('"name": "aphrody"');
  if (!hasMkt) {
    runCmd(["claude", "plugin", "marketplace", "add", REPO_ROOT]);
  } else {
    runCmd(["claude", "plugin", "marketplace", "update", "aphrody"]);
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

function setupAgyAndGemini(): void {
  const geminiRoot = join(HOME, ".gemini");
  // agy owns its plugins: ~/.gemini/scripts/install.ts builds the unified plugin (~/.gemini/plugins/aphrody) and
  // materializes it for agy and the Gemini CLI. Claude follows it (scripts/align-agy.ts), never the reverse.
  const agyInstall = join(geminiRoot, "scripts", "install.ts");
  if (existsSync(agyInstall)) {
    const res = runCmd(["bun", agyInstall]);
    if (!res.ok) console.error(`agy install: ${res.err || res.out}`);
  }
  const aligned = runCmd(["bun", join(REPO_ROOT, "scripts", "align-agy.ts")]);
  if (!aligned.ok) console.error(`align-agy: ${aligned.err || aligned.out}`);

  ensureDirLink(
    join(REPO_ROOT, "packages", "bun"),
    join(HOME, ".bun", "agent-plugin"),
  );

  const vars = templateVars();
  const discovered = discoverWorkspaces();
  const tmplRoot = join(REPO_ROOT, "gemini");
  const simpleFiles = [
    "settings.json",
    "trustedFolders.json",
    join("config", "hooks.json"),
    join("config", "mcp_config.json"),
    join("config", "projects", "default-cli-project.json"),
    join("antigravity-cli", "hooks.json"),
  ];
  for (const rel of simpleFiles) {
    const src = join(tmplRoot, rel);
    if (!existsSync(src)) continue;
    const rendered = renderTemplate(
      readFileSync(src, "utf8"),
      vars,
      rel.endsWith(".json"),
    );
    writeIfChanged(join(geminiRoot, rel), rendered);
  }

  // Generate clean case-deduplicated projects.json matching Gemini CLI's canonical path format.
  const geminiProjects: Record<string, string> = {};
  for (const [k, v] of Object.entries(discovered)) {
    const canonical =
      process.platform === "win32" ? k.replaceAll("/", "\\").toLowerCase() : k;
    geminiProjects[canonical] = v;
  }
  writeIfChanged(
    join(geminiRoot, "projects.json"),
    `${JSON.stringify({ projects: geminiProjects }, null, 2)}\n`,
  );

  // Purge unrelated third-party sidecars while preserving Antigravity App userSettings & live voice/swarm state.
  const agyConfigPath = join(geminiRoot, "config", "config.json");
  if (existsSync(agyConfigPath)) {
    try {
      const cfg = JSON.parse(readFileSync(agyConfigPath, "utf8")) as {
        sidecars?: Record<string, unknown>;
        [k: string]: unknown;
      };
      if (cfg.sidecars && typeof cfg.sidecars === "object") {
        let changed = false;
        for (const staleKey of [
          "android-cli-plugin/compose-preview",
          "android-cli-plugin/device-hub",
          "science/biomolecular_structure_viewer",
        ]) {
          if (staleKey in cfg.sidecars) {
            delete cfg.sidecars[staleKey];
            changed = true;
          }
        }
        if (changed) {
          writeIfChanged(agyConfigPath, `${JSON.stringify(cfg, null, 2)}\n`);
        }
      }
    } catch {
      // Preserve unreadable config untouched.
    }
  }

  const agySettingsSrc = join(tmplRoot, "antigravity-cli", "settings.json");
  const agySettingsDst = join(geminiRoot, "antigravity-cli", "settings.json");
  if (existsSync(agySettingsSrc)) {
    const desired = JSON.parse(
      renderTemplate(readFileSync(agySettingsSrc, "utf8"), vars, true),
    ) as Record<string, unknown>;
    let current: Record<string, unknown> = {};
    if (existsSync(agySettingsDst)) {
      try {
        current = JSON.parse(readFileSync(agySettingsDst, "utf8")) as Record<
          string,
          unknown
        >;
      } catch {
        current = {};
      }
    }
    const oldWs = Array.isArray(current.trustedWorkspaces)
      ? (current.trustedWorkspaces as string[])
      : [];
    const discoveredWin = Object.keys(discovered)
      .filter((p) => !p.endsWith("/.claude"))
      .map((p) => p.replaceAll("/", "\\"));
    const wantedWs = Array.isArray(desired.trustedWorkspaces)
      ? (desired.trustedWorkspaces as string[])
      : [];
    const seen = new Set<string>();
    const cleanWs: string[] = [];
    for (const raw of [...wantedWs, ...discoveredWin, ...oldWs]) {
      const win = raw.replaceAll("/", "\\");
      const key = win.toLowerCase();
      if (!existsSync(win) || seen.has(key)) continue;
      seen.add(key);
      cleanWs.push(win);
    }
    const merged = { ...current, ...desired, trustedWorkspaces: cleanWs };
    writeIfChanged(agySettingsDst, `${JSON.stringify(merged, null, 2)}\n`);
  }

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
  const geminiOnly = Bun.argv.includes("--gemini-only");
  if (!geminiOnly) {
    runCmd([
      "bun",
      "--no-install",
      join(REPO_ROOT, "scripts", "merge-brain.ts"),
      "--cleanup",
    ]);
    // Hostinger stdio MCP servers (token from HOSTINGER_API_TOKEN, no OAuth); version pinned, referenced by absolute path in the plugin.
    runCmd(["bun", "add", "-g", "@hostinger/mcp@4.3.0"]);
    setupGit();
  }
  setupAgyAndGemini();
  if (!geminiOnly) {
    await setupClaudePlugins();
  }
  syncInstructionFiles();
  runCmd(["bun", "--no-install", join(REPO_ROOT, "scripts", "merge-brain.ts")]);
  console.log("Setup complete.");
}
