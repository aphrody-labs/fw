#!/usr/bin/env bun
// Gate of the Kotlin workspace (kotlin/, the Aphrody Compose theme): the Gradle build runs on the build host, never on the
// machine that calls this tool. The working tree (kotlin/, brand/tokens, ay.toml, Cargo.toml) is streamed over ssh into a scratch
// directory of the build host's work volume and `gradlew test assembleRelease publishToMavenLocal` runs there against androidx material3.
//   bun tools/compose-check.ts [gradle tasks...]      defaults to: test assembleRelease publishToMavenLocal
// Host alias and layout come from ay.toml [hosts] / [paths.build] (work, optional android_sdk and gradle_home), so no host path is
// tracked. On a CI runner (CI=true) the build runs in place.
import { spawnSync } from "node:child_process";
import { hostAlias, hostWork, loadHosts } from "./hosts.ts";
import { repoRoot } from "./local.ts";

const inputs = ["kotlin", "brand/tokens", "ay.toml", "Cargo.toml"];
const tasks = process.argv.slice(2).length ? process.argv.slice(2) : ["test", "assembleRelease", "publishToMavenLocal"];

/** Files to ship: tracked and untracked-but-not-ignored, so an uncommitted change is what gets built. */
export function shipList(root = repoRoot): string[] {
  const r = spawnSync("git", ["ls-files", "-co", "--exclude-standard", "--", ...inputs], { cwd: root, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`git ls-files failed: ${r.stderr}`);
  return r.stdout.split("\n").filter(Boolean);
}

const q = (s: string): string => `'${s.replaceAll("'", `'\\''`)}'`;

export function remoteScript(dir: string, paths: { android_sdk?: string; gradle_home?: string }, work: string, gradleTasks: string[]): string {
  const env = [
    `export GRADLE_USER_HOME=${q(paths.gradle_home ?? `${work}/gradle-home`)}`,
    ...(paths.android_sdk ? [`export ANDROID_HOME=${q(paths.android_sdk)}`] : []),
    // dl.google.com answers 404 over IPv6 from some hosts
    "export JAVA_TOOL_OPTIONS=-Djava.net.preferIPv4Stack=true",
  ];
  return [
    "set -euo pipefail",
    ...env,
    `rm -rf ${q(dir)} && mkdir -p ${q(dir)}`,
    `tar -xf - -C ${q(dir)}`,
    `cd ${q(dir)}/kotlin`,
    `bash ./gradlew --no-daemon --console=plain ${gradleTasks.map(q).join(" ")}`,
  ].join("\n");
}

if (import.meta.main) {
  if (Bun.env.CI === "true") {
    const r = spawnSync("bash", ["./gradlew", "--no-daemon", "--console=plain", ...tasks], { cwd: `${repoRoot}/kotlin`, stdio: "inherit" });
    process.exit(r.status ?? 1);
  }
  const cfg = loadHosts();
  const alias = hostAlias("build", cfg);
  const work = hostWork("build", cfg);
  const tar = Bun.spawn(["tar", "-cf", "-", ...shipList()], { cwd: repoRoot, stdout: "pipe", stderr: "inherit" });
  const script = remoteScript(`${work}/ay-kotlin-check`, cfg.paths.build ?? {}, work, tasks);
  const ssh = Bun.spawn(["ssh", alias, `bash -c ${q(script)}`], { stdin: tar.stdout, stdout: "inherit", stderr: "inherit" });
  process.exit(await ssh.exited);
}
