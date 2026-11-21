#!/usr/bin/env bun
// Imports sibling repositories into the fw monorepo with their full history, each under its own top-level directory.
//   bun tools/monorepo/import.ts [--work /srv/xbuild/merge] [--repos aphrody,ay] [--push]
// Steps per repo:
//   1. fresh clone of origin;
//   2. `git filter-repo --to-subdirectory-filter <repo> --tag-rename :<repo>/`, which rewrites every path and tag;
//   3. merge into a scratch clone of fw with --allow-unrelated-histories.
// Then:
//   - root .gitmodules gains the imported submodules with prefixed paths (relative URLs stay valid in the same org);
//   - the root Cargo workspace excludes the imported trees, so each keeps building as its own workspace until the
//     measured unification steps;
//   - path dependencies on `../fw/...` are rewritten to the in-repo location.
// Re-runnable: filter-repo is deterministic, so a later run merges only commits made upstream since the last import.
// Gate printed at the end:
//   - commit counts (merged >= fw + each source);
//   - `cargo metadata` resolving for every workspace;
//   - the tree has no `../fw/` path dependency left.
import { $ } from "bun";
import { existsSync } from "node:fs";
import { join } from "node:path";

const flag = (name: string, fallback: string) => {
  const index = Bun.argv.indexOf(name);
  return index >= 0 && Bun.argv[index + 1] ? Bun.argv[index + 1] : fallback;
};
const work = flag("--work", `${process.env.WORK ?? "/srv/xbuild"}/merge`);
const repos = flag("--repos", "aphrody,ay").split(",").filter(Boolean);
const org = "https://github.com/aphrody-labs";
const fw = join(work, "fw");

await $`mkdir -p ${work}`;
if (!existsSync(fw)) await $`git clone --no-tags ${org}/fw.git ${fw}`;
await $`git -C ${fw} fetch -q origin && git -C ${fw} checkout -q main && git -C ${fw} merge -q --ff-only origin/main`;

const count = async (dir: string, ref = "HEAD") => Number((await $`git -C ${dir} rev-list --count ${ref}`.text()).trim());
const counts: Record<string, number> = { fw: await count(fw) };

for (const repo of repos) {
  const source = join(work, `src-${repo}`);
  await $`rm -rf ${source}`;
  await $`git clone --no-local ${org}/${repo}.git ${source}`;
  counts[repo] = await count(source);
  await $`git -C ${source} filter-repo --force --to-subdirectory-filter ${repo} --tag-rename ${`:${repo}/`}`;
  const remote = `import-${repo}`;
  await $`git -C ${fw} remote remove ${remote}`.nothrow().quiet();
  await $`git -C ${fw} remote add ${remote} ${source}`;
  await $`git -C ${fw} fetch -q ${remote} main --tags`;
  const merged = (await $`git -C ${fw} merge-base --is-ancestor ${remote}/main HEAD`.nothrow()).exitCode === 0;
  if (!merged) {
    await $`git -C ${fw} merge --allow-unrelated-histories --no-edit -m ${`monorepo: import ${repo} with full history under ${repo}/`} ${remote}/main`;
  }
}

// Submodules: move each imported .gitmodules entry to the root file with a prefixed path.
const rootModules = join(fw, ".gitmodules");
let modules = existsSync(rootModules) ? await Bun.file(rootModules).text() : "";
for (const repo of repos) {
  const nested = join(fw, repo, ".gitmodules");
  if (!existsSync(nested)) continue;
  const text = (await Bun.file(nested).text())
    .replace(/\[submodule "([^"]+)"\]/g, (_, name) => `[submodule "${repo}/${name}"]`)
    .replace(/^(\s*path\s*=\s*)(.+)$/gm, (_, lead, path) => `${lead}${repo}/${path.trim()}`);
  if (!modules.includes(`"${repo}/`)) modules = `${modules.trimEnd()}\n${text.trim()}\n`;
  await $`git -C ${fw} rm -q --cached ${`${repo}/.gitmodules`}`;
  await $`rm -f ${nested}`;
}
await Bun.write(rootModules, modules.trimStart());
await $`git -C ${fw} add .gitmodules`;

// Cargo: keep imported workspaces independent for now (one variable at a time).
const rootCargo = join(fw, "Cargo.toml");
let cargo = await Bun.file(rootCargo).text();
const excludeLine = cargo.match(/^exclude\s*=\s*\[([^\]]*)\]/m);
const wanted = repos.filter((repo) => existsSync(join(fw, repo, "Cargo.toml")));
if (excludeLine) {
  const current = [...excludeLine[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  const next = [...new Set([...current, ...wanted])];
  cargo = cargo.replace(excludeLine[0], `exclude = [${next.map((entry) => `"${entry}"`).join(", ")}]`);
} else {
  cargo = cargo.replace(/^\[workspace\]\s*$/m, `[workspace]\nexclude = [${wanted.map((entry) => `"${entry}"`).join(", ")}]`);
}
await Bun.write(rootCargo, cargo);
await $`git -C ${fw} add Cargo.toml`;

// Path dependencies that pointed at the sibling fw checkout now point inside the monorepo.
// `:(glob)` makes `**/` match zero or more directories, so `<repo>/Cargo.toml` itself is included.
const offenders = (await $`git -C ${fw} grep -l -E ${'path *= *"\\.\\./fw/'} -- ${repos.map((repo) => `:(glob)${repo}/**/Cargo.toml`)}`.nothrow().text()).split("\n").filter(Boolean);
for (const file of offenders) {
  const depth = file.split("/").length - 1;
  const up = "../".repeat(depth);
  const text = (await Bun.file(join(fw, file)).text()).replace(/(path\s*=\s*")\.\.\/fw\//g, `$1${up}`);
  await Bun.write(join(fw, file), text);
  await $`git -C ${fw} add ${file}`;
}
const staged = (await $`git -C ${fw} diff --cached --name-only`.text()).trim();
if (staged) await $`git -C ${fw} commit -q -m ${"monorepo: root .gitmodules, Cargo exclude and in-repo paths for imported trees"}`;

// Gate.
const total = await count(fw);
const sum = Object.values(counts).reduce((a, b) => a + b, 0);
const metadata: Record<string, string> = {};
for (const dir of [".", ...wanted]) {
  const result = await $`cargo metadata --format-version 1 --no-deps --offline`.cwd(join(fw, dir)).nothrow().quiet();
  metadata[dir] = result.exitCode === 0 ? `ok (${JSON.parse(result.stdout.toString()).packages.length} packages)` : `FAIL: ${result.stderr.toString().split("\n").find(Boolean)}`;
}
const leftover = (await $`git -C ${fw} grep -c -E ${'path *= *"\\.\\./fw/'}`.nothrow().text()).trim();
console.log(JSON.stringify({ counts, merged: total, historyPreserved: total >= sum, metadata, leftoverFwPaths: leftover || "0", head: (await $`git -C ${fw} rev-parse --short HEAD`.text()).trim() }, null, 2));

if (Bun.argv.includes("--push")) await $`git -C ${fw} push origin HEAD:refs/heads/main`;
