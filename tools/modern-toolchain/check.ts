#!/usr/bin/env bun
// Gates of the modern-toolchain lot, driven by tools/modern-toolchain/steps.toml.
//   bun tools/modern-toolchain/check.ts ignore    tracked files must not match an ignore rule or be build output;
//                                                 a Dockerfile needs a .dockerignore that excludes .git, target, node_modules
//   bun tools/modern-toolchain/check.ts legacy    no removed legacy tool in CI, scripts or container files
//   bun tools/modern-toolchain/check.ts userland  every adopted CLI exists on every node of cluster/build.toml (--local: and on this machine)
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
type Steps = {
  tracked_binary?: { path: string; reason: string }[];
  legacy?: { pattern: string; replacement: string; scope: string[] }[];
  userland?: { bin: string; replaces: string; status: "adopted" | "rejected" }[];
};
const steps = Bun.TOML.parse(await Bun.file(join(import.meta.dir, "steps.toml")).text()) as Steps;

const git = async (...args: string[]) => {
  const run = Bun.spawn(["git", "-C", root, ...args], { stdout: "pipe", stderr: "pipe" });
  const [out, code] = await Promise.all([new Response(run.stdout).text(), run.exited]);
  if (code !== 0) throw new Error(`git ${args.join(" ")} exited ${code}`);
  return out.split("\n").filter(Boolean);
};

const problems: string[] = [];

async function ignore() {
  for (const path of await git("ls-files", "-ci", "--exclude-standard")) problems.push(`tracked but ignored: ${path}`);
  const allowed = (steps.tracked_binary ?? []).map((entry) => entry.path);
  const output = /(^|\/)(target|node_modules|\.venv|__pycache__|\.gradle|\.kotlin|\.wrangler|\.next)\/|\.(exe|dll|pdb|o|obj|rlib|pyc|class)$/;
  for (const path of await git("ls-files")) if (output.test(path) && !allowed.includes(path)) problems.push(`tracked build output: ${path}`);
  const tracked = new Set(await git("ls-files"));
  if (tracked.has("Dockerfile")) {
    if (!tracked.has(".dockerignore")) problems.push("Dockerfile without .dockerignore");
    else {
      const lines = (await Bun.file(join(root, ".dockerignore")).text()).split("\n").map((line) => line.trim());
      for (const need of [".git", "target", "node_modules"]) if (!lines.includes(need)) problems.push(`.dockerignore lacks ${need}`);
    }
  }
}

async function legacy() {
  const files = await git("ls-files");
  for (const rule of steps.legacy ?? []) {
    const pattern = new RegExp(rule.pattern);
    for (const path of files) {
      if (path.startsWith("tools/modern-toolchain/") || !rule.scope.some((scope) => path === scope || path.startsWith(`${scope}/`))) continue;
      const text = await Bun.file(join(root, path)).text().catch(() => "");
      text.split("\n").forEach((line, index) => {
        if (pattern.test(line)) problems.push(`${path}:${index + 1}: ${line.trim().slice(0, 100)} -> use ${rule.replacement}`);
      });
    }
  }
}

async function userland() {
  const adopted = (steps.userland ?? []).filter((entry) => entry.status === "adopted");
  const targets: { name: string; argv: (bin: string) => string[] }[] = [];
  if (process.argv.includes("--local")) targets.push({ name: "local", argv: (bin: string) => ["bash", "-c", `command -v ${bin}`] });
  {
    const build = Bun.TOML.parse(await Bun.file(join(root, "cluster", "build.toml")).text()) as { hosts?: Record<string, { ssh?: string }> };
    const names = Object.entries(build.hosts ?? {}).map(([name, host]) => host.ssh ?? name);
    for (const name of names) targets.push({ name, argv: (bin: string) => ["ssh", "-o", "BatchMode=yes", name, `command -v ${bin}`] });
  }
  for (const target of targets) {
    for (const entry of adopted) {
      const run = Bun.spawn(target.argv(entry.bin), { stdout: "pipe", stderr: "ignore", stdin: "ignore" });
      if ((await run.exited) !== 0) problems.push(`${target.name}: ${entry.bin} missing (replaces ${entry.replaces})`);
    }
  }
}

const command = process.argv[2];
if (command === "ignore") await ignore();
else if (command === "legacy") await legacy();
else if (command === "userland") await userland();
else {
  console.error("usage: check.ts ignore | legacy | userland [--local]");
  process.exit(2);
}
if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log(`${command}: ok`);
