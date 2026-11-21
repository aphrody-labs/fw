#!/usr/bin/env bun
// Checks the tree against tools/layout.toml: nothing in the legacy src/ or upstream/, every crate of the lock where the layout puts it,
// every first-party crate marked as such, every fork pinned by a full commit and none checked out as a submodule. Prints the problems, exit 1 if there are any.
//   bun tools/layout.ts [repo]
import { join } from "node:path";

const repo = Bun.argv[2] ?? ".";
const at = (path: string) => join(repo, path);
const layout = Bun.TOML.parse(await Bun.file(at("tools/layout.toml")).text()) as {
  forks: Record<string, { repo: string; commit: string }>;
  first_party: { dir: string };
  third_party: { dir: string };
};
const lock = (await Bun.file(at("tools/upstreams.lock.json")).json()) as Record<
  string,
  { crates: Record<string, string> }
>;
const tracked = async (...paths: string[]) =>
  (await Bun.$`git -C ${repo} ls-files -z -- ${paths}`.text()).split("\0").filter(Boolean);
const problems: string[] = [];

for (const legacy of await tracked("src", "upstream")) problems.push(`legacy location: ${legacy}`);

for (const [name, entry] of Object.entries(lock))
  for (const dir of Object.values(entry.crates)) {
    if (!dir.startsWith(`${layout.third_party.dir}/${name}/`))
      problems.push(`${name}: crate at ${dir}, expected under ${layout.third_party.dir}/${name}/`);
    else if (!(await Bun.file(at(`${dir}/Cargo.toml`)).exists())) problems.push(`${name}: no manifest at ${dir}`);
  }

for (const manifest of await tracked(`${layout.first_party.dir}/*/Cargo.toml`)) {
  if (!(await Bun.file(at(manifest)).exists())) continue;
  const meta = (
    Bun.TOML.parse(await Bun.file(at(manifest)).text()) as {
      package?: { metadata?: { browser?: { native?: boolean } } };
    }
  ).package?.metadata?.browser;
  if (meta?.native !== true)
    problems.push(`${manifest}: first-party crates set [package.metadata.browser] native = true`);
}

for (const [name, fork] of Object.entries(layout.forks ?? {})) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(fork.repo ?? "")) problems.push(`forks.${name}: repo must be owner/name`);
  if (!/^[0-9a-f]{40}$/.test(fork.commit ?? "")) problems.push(`forks.${name}: commit must be a full SHA`);
}
if (await Bun.file(at(".gitmodules")).exists())
  problems.push(".gitmodules: forks are pinned in tools/layout.toml [forks], not submodules");

console.log(problems.length ? problems.join("\n") : "layout ok");
process.exit(problems.length ? 1 : 0);
