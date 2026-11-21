// Shared by the tests: pick the generated module matching the host, or say why there is none.
import * as aarch64 from "../src/aarch64/index.ts";
import * as x86_64 from "../src/x86_64/index.ts";
import { hostArch } from "../src/index.ts";

export const host = hostArch();

// Both arch modules expose the same names; the tests only use names present in both.
export const kernel = host === "aarch64" ? aarch64 : x86_64;

export const skipReason: string | undefined =
  process.platform !== "linux" ? `host OS is ${process.platform}, not linux` : host === undefined ? `host arch ${process.arch} has no generated module (x86_64 and aarch64 only)` : undefined;

export function hostCommand(command: string[]): string {
  const result = Bun.spawnSync(command, { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(`${command.join(" ")} exited ${result.exitCode}: ${result.stderr.toString()}`);
  return result.stdout.toString().trim();
}
