#!/usr/bin/env bun
// material-cdn: build the WOFF2 encoder wasm used by the Worker (cloudflare/material-cdn/woff2, Rust) into cloudflare/material-cdn/wasm/woff2.wasm.
//   bun tools/material-cdn/wasm.ts [--target-dir DIR]
// Needs rustup target wasm32-unknown-unknown; runs wasm-opt -Oz when it is installed.
import { copyFile, mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const crate = resolve(import.meta.dir, "..", "..", "cloudflare", "material-cdn", "woff2");
const targetDir = resolve(args[args.indexOf("--target-dir") + 1] ?? join(crate, "target"));
const out = resolve(crate, "..", "wasm");

async function run(cmd: string[], cwd?: string, env: Record<string, string | undefined> = {}) {
  const p = Bun.spawn(cmd, { cwd, stdout: "inherit", stderr: "inherit", env: { ...process.env, ...env } });
  if ((await p.exited) !== 0) throw new Error(`${cmd.join(" ")} failed`);
}

await run(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown", "--locked"], crate, { CARGO_TARGET_DIR: targetDir });
const built = join(targetDir, "wasm32-unknown-unknown", "release", "material_cdn_woff2.wasm");
await mkdir(out, { recursive: true });
const dest = join(out, "woff2.wasm");
if (Bun.which("wasm-opt")) await run(["wasm-opt", "-Oz", "--enable-bulk-memory", "--enable-sign-ext", "--enable-nontrapping-float-to-int", "--enable-simd", "-o", dest, built]);
else await copyFile(built, dest);
console.log(`${dest} ${Bun.file(dest).size} bytes`);
