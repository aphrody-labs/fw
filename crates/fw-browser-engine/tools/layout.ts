#!/usr/bin/env bun
// The layout of the monorepo (tools/layout.toml) applied to the tree: `plan` prints the moves, `apply` makes them with
// `git mv` and rewrites the path dependencies of the manifests that moved. Idempotent: what is in place stays, so after an
// import it also places whatever the importer left in the legacy locations (src/<crate>, upstream/<name>).
//   bun tools/layout.ts [plan|apply|root] [repo]
// `root` rewrites the legacy src/<crate> strings of the root Cargo.toml (members, workspace dependencies, patches) to where
// each crate is now, found on disk; the rest of that file (profiles, unified requirements) is left as it is.
import { dirname, join, normalize, relative } from "node:path";

type Layout = {
  bun: { dir: string; workspace: string; saved: string[] };
  first_party: { dir: string; marker: string };
  third_party: { dir: string; external: string[] };
};
type Lock = Record<string, { crates: Record<string, string> }>;

const [command = "plan", repo = "."] = Bun.argv.slice(2);
const root = Bun.pathToFileURL(join(process.cwd(), repo, "/")).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const at = (path: string) => join(repo, path);
const layout = Bun.TOML.parse(await Bun.file(at("tools/layout.toml")).text()) as Layout;
const lock = (await Bun.file(at("tools/upstreams.lock.json")).json()) as Lock;
const git = async (...args: string[]) => (await Bun.$`git -C ${repo} ${args}`.text()).split("\n").filter(Boolean);
const posix = (path: string) => path.replaceAll("\\", "/");

// ---- what is where ------------------------------------------------------------------------------------------
const tracked = await git("ls-files", "-z", "--", "src", "upstream").then(async () =>
  (await Bun.$`git -C ${repo} ls-files -z -- src upstream`.text()).split("\0").filter(Boolean),
);
const top = (dir: string) => [...new Set(tracked.filter((p) => p.startsWith(dir + "/")).map((p) => p.split("/")[1]))];

const imported = new Map<string, string>(); // legacy crate dir (src/<leaf>) -> upstream name
for (const [name, entry] of Object.entries(lock))
  if (name !== "bun") for (const dir of Object.values(entry.crates)) imported.set(dir, name);

/** Reads `[package.metadata.browser] native = true` from a manifest without a TOML dependency on the whole file. */
async function firstParty(dir: string): Promise<boolean> {
  const file = Bun.file(at(`${dir}/Cargo.toml`));
  if (!(await file.exists())) return false;
  const meta = (Bun.TOML.parse(await file.text()) as any).package?.metadata?.browser;
  return meta?.native === true;
}

// ---- the moves: [from, to] over repository-relative paths ----------------------------------------------------
const moves: [string, string][] = [];
const bun = layout.bun.dir;
const third = layout.third_party.dir;

for (const name of top("upstream")) {
  if (name === "bun") {
    for (const file of tracked.filter((p) => p.startsWith("upstream/bun/")).map((p) => p.slice("upstream/bun/".length))) {
      moves.push([`upstream/bun/${file}`, file === "Cargo.toml" ? layout.bun.workspace : `${bun}/${file}`]);
    }
  } else moves.push([`upstream/${name}`, `${third}/${name}`]);
}
for (const entry of top("src")) {
  const from = `src/${entry}`;
  const upstream = imported.get(from);
  if (upstream) moves.push([from, `${third}/${upstream}/${entry}`]);
  else if (await firstParty(from)) moves.push([from, `${layout.first_party.dir}/${entry}`]);
  else moves.push([from, `${bun}/src/${entry}`]);
}

// Longest-prefix translation of any legacy path, used to rewrite relative references.
const table = moves.toSorted((a, b) => b[0].length - a[0].length);
const translate = (path: string): string => {
  for (const [from, to] of table) if (path === from || path.startsWith(from + "/")) return to + path.slice(from.length);
  return path;
};
const legacy = (path: string): string => {
  for (const [from, to] of table.toSorted((a, b) => b[1].length - a[1].length))
    if (path === to || path.startsWith(to + "/")) return from + path.slice(to.length);
  return path;
};

if (command === "root") {
  const manifest = at("Cargo.toml");
  const exists = async (path: string) => Bun.file(at(path)).stat().then(() => true, () => false);
  const found = new Map<string, string>();
  const entries = new Set<string>();
  for (const m of (await Bun.file(manifest).text()).matchAll(/"src\/([^"\/]+)(?:\/[^"]*)?"/g)) entries.add(m[1]);
  const third = await Array.fromAsync(new Bun.Glob("*/*").scan({ cwd: at(layout.third_party.dir), onlyFiles: false }));
  for (const entry of entries) {
    const hit = third.find((p) => p.split("/")[1] === entry);
    if (hit) found.set(entry, `${layout.third_party.dir}/${hit}`);
    else if (await exists(`${layout.first_party.dir}/${entry}`)) found.set(entry, `${layout.first_party.dir}/${entry}`);
    else found.set(entry, `${layout.bun.dir}/src/${entry}`);
  }
  const text = (await Bun.file(manifest).text()).replace(/"src\/([^"\/]+)((?:\/[^"]*)?)"/g, (_, entry: string, rest: string) => `"${found.get(entry)}${rest}"`);
  await Bun.write(manifest, text);
  console.log(`root manifest: ${entries.size} crate paths relocated`);
  process.exit(0);
}
if (command === "plan") {
  for (const [from, to] of moves) console.log(`${from} -> ${to}`);
  console.log(`${moves.length} moves`);
  process.exit(0);
}
if (command !== "apply") throw new Error(`unknown command ${command}`);

// ---- apply ---------------------------------------------------------------------------------------------------
const targets = new Set(moves.map(([, to]) => to));
const stat = (path: string) => Bun.file(at(path)).stat().catch(() => null);
/** Moves `from` to `to`; when `to` is already there (an import added to a directory that was placed before) both are merged entry by entry. */
async function place(from: string, to: string) {
  const [source, target] = [await stat(from), await stat(to)];
  if (!source) return;
  if (!target) {
    await Bun.$`mkdir -p ${dirname(at(to))}`;
    await Bun.$`git -C ${repo} mv ${from} ${to}`.quiet();
  } else if (source.isDirectory() && target.isDirectory()) {
    for (const child of await Array.fromAsync(new Bun.Glob("*").scan({ cwd: at(from), onlyFiles: false, dot: true })))
      await place(`${from}/${child}`, `${to}/${child}`);
    await Bun.$`rmdir ${at(from)}`.nothrow().quiet();
  } else throw new Error(`destination exists: ${to} (from ${from})`);
}
for (const [from, to] of moves) await place(from, to);
console.log(`moved ${moves.length} entries`);

// Path dependencies of every manifest that moved: resolved in legacy coordinates, re-expressed from the new location.
const manifests = (await Bun.$`git -C ${repo} ls-files -z -- '*Cargo.toml'`.text()).split("\0").filter(Boolean);
let rewritten = 0;
for (const manifest of manifests) {
  const old = legacy(manifest);
  if (old === manifest && !manifest.startsWith(`${bun}/`)) continue; // did not move
  const text = await Bun.file(at(manifest)).text();
  const next = text.replace(/(\bpath\s*=\s*")((?:\.\.\/)+[^"]*)(")/g, (all, a: string, rel: string, c: string) => {
    const target = posix(normalize(join(dirname(old), rel))); // where it pointed, in legacy coordinates
    const moved = translate(target);
    if (moved === target) return all; // not a moved directory: the relative path keeps its meaning
    const fresh = posix(relative(dirname(manifest), moved));
    return fresh === rel ? all : `${a}${fresh}${c}`;
  });
  if (next !== text) {
    await Bun.write(at(manifest), next);
    rewritten++;
  }
}
console.log(`rewrote path dependencies in ${rewritten} manifests`);

// The lock records where each crate of an upstream lives.
for (const [name, entry] of Object.entries(lock))
  for (const [from, dir] of Object.entries(entry.crates))
    entry.crates[from] = dir.startsWith("src/") ? `${third}/${name}/${dir.slice(4)}` : translate(dir); // src/<leaf>: just imported
await Bun.write(at("tools/upstreams.lock.json"), JSON.stringify(lock, null, 1) + "\n");
console.log(`lock updated; ${targets.size} destinations`);
