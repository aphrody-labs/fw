#!/usr/bin/env bun
// SessionStart hook: refreshes the fw marketplace and the aphrody plugin at most once per interval. The check is a stat
// call; the update itself runs in a detached child, so the session never waits for git or the claude CLI.
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

const INTERVAL_MS = 6 * 60 * 60 * 1000;
const dataDir = process.env.CLAUDE_PLUGIN_DATA ?? join(tmpdir(), "aphrody-plugin");
const stamp = join(dataDir, "update.stamp");

async function update(): Promise<void> {
  const claudeDir = process.env.CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude");
  const marketplaces = Bun.file(join(claudeDir, "plugins", "known_marketplaces.json"));
  const entry = ((await marketplaces.exists()) ? ((await marketplaces.json()) as Record<string, { installLocation?: string; source?: { source?: string } }>) : {}).fw;
  const dir = entry?.installLocation;
  if (dir && entry?.source?.source === "directory" && existsSync(join(dir, ".git"))) {
    await Bun.$`git -C ${dir} pull --ff-only --quiet`.nothrow().quiet();
  }
  await Bun.$`claude plugin marketplace update fw`.nothrow().quiet();
  await Bun.$`claude plugin update aphrody@fw`.nothrow().quiet();
}

if (Bun.argv.includes("--run")) {
  await update();
} else {
  await Bun.stdin.stream().cancel();
  const age = existsSync(stamp) ? Date.now() - statSync(stamp).mtimeMs : Infinity;
  if (age > INTERVAL_MS) {
    mkdirSync(dataDir, { recursive: true });
    writeFileSync(stamp, "");
    utimesSync(stamp, new Date(), new Date());
    spawn(process.execPath, [import.meta.path, "--run"], { detached: true, stdio: "ignore", windowsHide: true }).unref();
  }
  process.stdout.write("{}");
}
