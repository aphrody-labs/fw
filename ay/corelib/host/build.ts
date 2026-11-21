// Build the artifacts the host needs: the corelib module from C (clang) and the apps from WAT (wasm-tools).
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { buildWasm, root } from "../../tools/corelib.ts";

export { root };


function sh(cmd: string[]): void {
  const p = Bun.spawnSync(cmd, { cwd: root, stdout: "pipe", stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")} failed (${p.exitCode}): ${p.stderr.toString()}`);
}

/** Compile the corelib sources to a Wasm module and return its bytes. */
export function buildCore(): Uint8Array {
  return new Uint8Array(readFileSync(buildWasm("base")));
}

/** Assemble WAT text to a binary module with wasm-tools. */
export function assemble(wat: string): Uint8Array {
  const dir = mkdtempSync(join(tmpdir(), "ay-wat-"));
  try {
    writeFileSync(join(dir, "in.wat"), wat);
    sh(["wasm-tools", "parse", join(dir, "in.wat"), "-o", join(dir, "out.wasm")]);
    return new Uint8Array(readFileSync(join(dir, "out.wasm")));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

export function assembleFile(path: string): Uint8Array {
  return assemble(readFileSync(join(root, path), "utf8"));
}
