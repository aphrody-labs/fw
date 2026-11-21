#!/usr/bin/env bun
// Gate of the Kotlin workspace (kotlin): Gradle build and tests run on the build host, never on this machine.
//   bun tools/kotlin-check.ts [--host <ssh alias>] [--ay <local ay checkout>] [gradle tasks...]   (defaults: FW_BUILD_HOST or fr1, test assembleRelease)
// What is shipped: kotlin, product.toml and the native/fw-abi crates (the fw-rust-ffi tests load the built libfw_abi.so), as the
// working tree stands. With --ay, that checkout's kotlin/, brand/tokens, ay.toml and Cargo.toml are shipped too and Gradle includes it
// (-Paphrody.ay.checkout), so an unpublished com.aphrody:aphrody-theme is substituted in place; without it the published artifact is
// resolved from the GitHub Packages registry named in product.toml (GITHUB_ACTOR / GITHUB_TOKEN of this shell are forwarded).
// Host-local settings (ANDROID_HOME, GRADLE_USER_HOME, CARGO_TARGET_DIR) live in ~/.config/fw/kotlin-build.env on the build host.
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const argv = process.argv.slice(2);
const take = (flag: string): string | undefined => {
  const i = argv.indexOf(flag);
  if (i < 0) return undefined;
  const [, value] = argv.splice(i, 2);
  return value;
};
const host = take("--host") ?? Bun.env.FW_BUILD_HOST ?? "fr1";
const ay = take("--ay");
const tasks = argv.length ? argv : ["test", "assembleRelease"];

const q = (s: string): string => `'${s.replaceAll("'", `'\\''`)}'`;

export function shipList(dir: string, inputs: string[]): string[] {
  const r = spawnSync("git", ["ls-files", "-co", "--exclude-standard", "--", ...inputs], { cwd: dir, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`git ls-files failed in ${dir}: ${r.stderr}`);
  return r.stdout.split("\n").filter(Boolean);
}

async function ship(dir: string, inputs: string[], remoteDir: string): Promise<void> {
  const tar = Bun.spawn(["tar", "-cf", "-", ...shipList(dir, inputs)], { cwd: dir, stdout: "pipe", stderr: "inherit" });
  const ssh = Bun.spawn(["ssh", host, `bash -c ${q(`mkdir -p ${remoteDir} && tar -xf - -C ${remoteDir}`)}`], { stdin: tar.stdout, stderr: "inherit" });
  if ((await ssh.exited) !== 0) throw new Error(`shipping ${dir} to ${host} failed`);
}

export function remoteScript(gradleTasks: string[], withAy: boolean, env: Record<string, string>): string {
  return [
    "set -euo pipefail",
    "D=$HOME/fw-kotlin-check",
    "[ -f ~/.config/fw/kotlin-build.env ] && . ~/.config/fw/kotlin-build.env",
    "export JAVA_TOOL_OPTIONS=-Djava.net.preferIPv4Stack=true",
    ...Object.entries(env).map(([k, v]) => `export ${k}=${q(v)}`),
    `cargo build --release --manifest-path "$D/native/fw-abi/Cargo.toml"`,
    `export FW_ABI_PATH="\${CARGO_TARGET_DIR:-$D/native/fw-abi/target}/release/libfw_abi.so"`,
    `cd "$D/core/kotlin"`,
    `bash ./gradlew --no-daemon --console=plain ${withAy ? '-Paphrody.ay.checkout="$D/ay" ' : ""}${gradleTasks.map(q).join(" ")}`,
  ].join("\n");
}

if (import.meta.main) {
  const wipe = spawnSync("ssh", [host, `bash -c ${q('rm -rf "$HOME/fw-kotlin-check" && mkdir -p "$HOME/fw-kotlin-check"')}`], { stdio: "inherit" });
  if (wipe.status !== 0) process.exit(wipe.status ?? 1);
  await ship(root, ["kotlin", "product.toml", "native/fw-abi"], '"$HOME/fw-kotlin-check"');
  if (ay) await ship(resolve(ay), ["kotlin", "brand/tokens", "ay.toml", "Cargo.toml"], '"$HOME/fw-kotlin-check/ay"');
  const forwarded = Object.fromEntries(["GITHUB_ACTOR", "GITHUB_TOKEN"].flatMap((k) => (Bun.env[k] ? [[k, Bun.env[k] as string]] : [])));
  const run = spawnSync("ssh", [host, `bash -c ${q(remoteScript(tasks, Boolean(ay), forwarded))}`], { stdio: "inherit" });
  process.exit(run.status ?? 1);
}
