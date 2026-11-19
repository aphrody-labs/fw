#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

function resolveFairwindRoot(): string {
  const candidates = [
    process.env.FAIRWIND_ROOT,
    resolve(dirname(import.meta.path), "..", ".."),
    "C:\\src\\fairwind",
    process.cwd(),
  ].filter((v): v is string => Boolean(v));
  for (const dir of candidates) {
    if (existsSync(join(dir, "fairwind.targets.json")) || existsSync(join(dir, "src", "fairwind_cli.ts"))) {
      return resolve(dir);
    }
  }
  return resolve(process.cwd());
}

const root = resolveFairwindRoot();
const isWin = process.platform === "win32";
const exeName = isWin ? "fairwind.exe" : "fairwind";
const targetBin = join(root, "target", "agy", exeName);
const distBin = join(root, "dist", exeName);
const cliEntry = join(root, "src", "fairwind_cli.ts");

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
  env: { ...process.env, FAIRWIND_ROOT: root },
});

process.exit(await proc.exited);
