#!/usr/bin/env bun
// Puts a checkout on a build host the fast way. Streaming the working tree through tar/ssh took 10 minutes for a large repository and
// re-sent everything each time; this sends only what the host cannot already fetch:
//   - the committed state: the host fetches that commit from the repository's own remote (shallow, incremental, on the host's fast link);
//   - the uncommitted state: `git diff HEAD --binary` and the untracked files, usually a few kilobytes.
// The commit must be pushed (the host cannot fetch what only exists on this machine); build outputs on the host (target, node_modules,
// .cache) are kept between runs.
//   bun tools/sync-host.ts <checkout dir> [--role build] [--to <directory on the host>]
// The host is the ssh alias of the role and the destination defaults to <work>/sync/<repo>, both from ay.toml / ay.local.toml (tools/hosts.ts).
import { basename, resolve } from "node:path";
import { hostAlias, hostWork, loadHosts, type Role } from "./hosts.ts";

export const KEEP = ["target", "node_modules", ".cache", "build", "out"];

export type Checkout = { dir: string; sha: string; remote: string; patch: Uint8Array; untracked: string[]; pushed: boolean };

const git = (dir: string, ...args: string[]) => {
  const run = Bun.spawnSync(["git", "-C", dir, ...args], { stdout: "pipe", stderr: "pipe" });
  return { code: run.exitCode ?? 1, out: run.stdout, err: run.stderr.toString().trim() };
};
const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes).trim();

export function inspect(dir: string): Checkout {
  const sha = text(git(dir, "rev-parse", "HEAD").out);
  const remote = text(git(dir, "remote", "get-url", "origin").out);
  if (!sha || !remote) throw new Error(`${dir} is not a git checkout with an origin`);
  const pushed = text(git(dir, "branch", "-r", "--contains", sha).out) !== "";
  const patch = git(dir, "diff", "HEAD", "--binary").out;
  const untracked = text(git(dir, "ls-files", "--others", "--exclude-standard", "-z").out).split("\0").filter(Boolean);
  return { dir, sha, remote, patch, untracked, pushed };
}

/** The shell script the host runs: fetch the commit, discard what a previous sync left, then apply the uncommitted state read from the tar stream. */
export function remoteScript(destination: string, sha: string, remote: string, hasPatch: boolean, hasBody = hasPatch): string {
  const keep = KEEP.map((name) => `-e ${name}`).join(" ");
  return [
    "set -eu",
    `mkdir -p ${JSON.stringify(destination)} && cd ${JSON.stringify(destination)}`,
    "[ -d .git ] || git init -q .",
    `git remote get-url origin >/dev/null 2>&1 && git remote set-url origin ${JSON.stringify(remote)} || git remote add origin ${JSON.stringify(remote)}`,
    `git fetch -q --depth 1 origin ${sha}`,
    "git checkout -q --force FETCH_HEAD",
    `git clean -fdq ${keep}`,
    ...(hasBody ? ["tar xf -"] : []),
    hasPatch ? "[ -f .git/.sync.patch ] && mv .git/.sync.patch .sync.patch; git apply --binary .sync.patch && rm -f .sync.patch" : "rm -f .sync.patch .git/.sync.patch",
    "git rev-parse --short HEAD",
  ].join("\n");
}

/** A tar of the untracked files plus the uncommitted diff as .sync.patch (staged in .git, so it never shows as untracked). */
async function stream(checkout: Checkout): Promise<Uint8Array> {
  const patch = `${checkout.dir}/.git/.sync.patch`;
  if (checkout.patch.length) await Bun.write(patch, checkout.patch);
  const files = [...checkout.untracked, ...(checkout.patch.length ? [".git/.sync.patch"] : [])];
  if (!files.length) return new Uint8Array();
  const packed = Bun.spawnSync(["tar", "-cf", "-", "--null", "-T", "-"], { cwd: checkout.dir, stdin: new TextEncoder().encode(files.join("\0")), stdout: "pipe", stderr: "pipe" });
  if (packed.exitCode !== 0) throw new Error(`tar failed: ${packed.stderr.toString().trim()}`);
  return packed.stdout;
}

/** The ssh command line: the script travels as an argument (base64, so no quoting), leaving stdin to the tar stream the script reads. */
export function sshArguments(host: string, script: string): string[] {
  const encoded = Buffer.from(script).toString("base64");
  return ["-o", "BatchMode=yes", host, `bash -c "$(echo ${encoded} | base64 -d)" 2>&1 | grep -v Pseudo`];
}

if (import.meta.main) {
  const argv = Bun.argv.slice(2);
  const dir = resolve(argv.find((arg, index) => !arg.startsWith("--") && !argv[index - 1]?.startsWith("--")) ?? ".");
  const flag = (name: string) => argv[argv.indexOf(`--${name}`) + 1];
  const role = (argv.includes("--role") ? flag("role") : "build") as Role;
  const config = loadHosts();
  const host = hostAlias(role, config);
  const destination = argv.includes("--to") ? flag("to")! : `${hostWork(role, config)}/sync/${basename(dir)}`;
  const checkout = inspect(dir);
  if (!checkout.pushed) throw new Error(`HEAD ${checkout.sha.slice(0, 9)} is not on a remote branch: push it first`);
  const started = performance.now();
  const body = await stream(checkout);
  const script = remoteScript(destination, checkout.sha, checkout.remote, checkout.patch.length > 0, body.length > 0);
  const run = Bun.spawnSync(["ssh", ...sshArguments(host, script)], { stdin: body.length ? body : "ignore", stdout: "pipe", stderr: "pipe" });
  console.log(`${basename(dir)} -> ${role}:${destination} ${text(run.stdout).split("\n").slice(-1)[0]} (${checkout.patch.length} B patch, ${checkout.untracked.length} untracked, ${Math.round(performance.now() - started)} ms)`);
  if (run.exitCode !== 0) throw new Error(text(run.stderr) || text(run.stdout));
}
