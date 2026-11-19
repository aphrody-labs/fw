#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

function resolveFwRoot(): string {
  const candidates = [
    process.env.FW_ROOT,
    process.env.FAIRWIND_ROOT, // legacy fallback for units installed before the rename
    resolve(dirname(import.meta.path), "..", ".."),
    "C:\\src\\fw",
    process.cwd(),
  ].filter((v): v is string => Boolean(v));
  for (const dir of candidates) {
    if (existsSync(join(dir, "fw.targets.json")) || existsSync(join(dir, "src", "fw_cli.ts"))) {
      return resolve(dir);
    }
  }
  return resolve(process.cwd());
}

const root = resolveFwRoot();
const isWin = process.platform === "win32";
const exeName = isWin ? "fw.exe" : "fw";
const targetBin = join(root, "target", "agy", exeName);
const distBin = join(root, "dist", exeName);
const cliEntry = join(root, "src", "fw_cli.ts");

const cmd = existsSync(targetBin)
  ? [targetBin, "mcp"]
  : existsSync(distBin)
    ? [distBin, "mcp"]
    : [process.execPath, cliEntry, "mcp"];

const proc = Bun.spawn(cmd, {
  cwd: root,
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
  env: { ...process.env, FW_ROOT: root },
});

process.exit(await proc.exited);
