#!/usr/bin/env bun
// Renders MAP.tsv (one row per top-level directory: files, bytes, gate) with the map tool of the organization standard repository.
// The checkout comes from [repos.standard] checkout of ay.toml / ay.local.toml; without one the step is skipped.
//   bun tools/map.ts [--check]
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { readAy, repoRoot } from "./local.ts";

/** The map tool of the standard checkout, or undefined when no checkout is configured or present. */
export function mapTool(root = repoRoot): string | undefined {
  const hint = (readAy(root) as Record<string, any>).repos?.standard?.checkout as string | undefined;
  const tool = hint ? join(resolve(root, hint), "tools", "map.ts") : undefined;
  return tool && existsSync(tool) ? tool : undefined;
}

if (import.meta.main) {
  const tool = mapTool();
  if (!tool) {
    console.log("skipped: no standard checkout ([repos.standard] checkout)");
    process.exit(0);
  }
  const check = Bun.argv.includes("--check") ? ["--check"] : [];
  const r = Bun.spawnSync(["bun", tool, "--root", repoRoot, ...check], { stdout: "inherit", stderr: "inherit" });
  process.exit(r.exitCode ?? 1);
}
