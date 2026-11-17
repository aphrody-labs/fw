// Compiles a C/C++ wrapper to a shared library on first use and returns its path.
//
// The cache directory is derived from os.tmpdir() and from a hash of the source, the compiler and the
// link flags, so the library is rebuilt only when one of them changes. The build is written to a
// temporary name and renamed, so concurrent importers never load a half-written file.
// Compilers can be overridden with AY_FFI_CC (C) and AY_FFI_CXX (C++).

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, extname, join } from "node:path";
import { fileURLToPath } from "node:url";

export type NativeBuild = {
  /** Absolute path of the C (.c) or C++ (.cpp) source. */
  source: string;
  /** Linker flags, e.g. ["-lcurl"]. */
  libs: string[];
  /** Name of the package that asks for the build, used in error messages. */
  owner: string;
};

export function buildSharedLibrary({ source, libs, owner }: NativeBuild): string {
  const cxx = extname(source) !== ".c";
  const compiler = cxx ? (process.env.AY_FFI_CXX ?? "g++") : (process.env.AY_FFI_CC ?? "gcc");
  const hash = createHash("sha1")
    .update(readFileSync(source))
    .update(compiler)
    .update(libs.join(" "))
    .digest("hex")
    .slice(0, 12);

  const dir = join(tmpdir(), "ay-ffi", hash);
  const out = join(dir, `${basename(source, extname(source))}.so`);
  if (existsSync(out)) return out;

  mkdirSync(dir, { recursive: true });
  const staging = `${out}.${process.pid}.tmp`;
  const args = ["-shared", "-fPIC", "-O2", "-o", staging, source, ...libs];
  if (cxx) args.push("-lstdc++");
  const result = Bun.spawnSync([compiler, ...args]);
  if (result.exitCode !== 0) {
    rmSync(staging, { force: true });
    const stderr = new TextDecoder().decode(result.stderr);
    throw new Error(`${owner}: failed to compile ${basename(source)} with ${compiler}:\n${stderr}`);
  }
  renameSync(staging, out);
  return out;
}

/** Absolute path of a source file shipped in this package's `native/` directory. */
export function nativeSource(file: string): string {
  return fileURLToPath(new URL(`../native/${file}`, import.meta.url));
}
