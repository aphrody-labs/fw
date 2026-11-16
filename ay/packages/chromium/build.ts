// Builds the Rust crate ay_chromium (addon + helper executable) and places them in native/.
// Runs on the build host: cargo needs CEF, which cef-dll-sys downloads on first build (CEF_PATH caches it).
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const run = (cmd: string[]) => {
  const p = Bun.spawnSync(cmd, { cwd: root, stdout: "inherit", stderr: "inherit", env: { ...process.env, CARGO_INCREMENTAL: "0" } });
  if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")} exited with ${p.exitCode}`);
};

// Two invocations: the Node-API symbols resolve only when Bun loads the addon, so the helper executable must be
// linked without the `node` feature.
run(["cargo", "build", "--release", "-p", "ay_chromium", "--bins"]);
run(["cargo", "build", "--release", "-p", "ay_chromium", "--lib", "--features", "node"]);

const meta = Bun.spawnSync(["cargo", "metadata", "--format-version", "1", "--no-deps"], { cwd: root });
const target: string = JSON.parse(meta.stdout.toString()).target_directory;
const native = join(import.meta.dir, "native");
mkdirSync(native, { recursive: true });
copyFileSync(join(target, "release", "libay_chromium.so"), join(native, "ay_chromium.node"));
copyFileSync(join(target, "release", "ay-chromium"), join(native, "ay-chromium"));
console.log(`built ${native}`);
