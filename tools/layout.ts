#!/usr/bin/env bun
// layout.ts: one declaration per repository (layout.toml) from which the native manifests of every ecosystem are
// written, so a move, rename, repository switch or patch edits that declaration and nothing else.
//
//   bun tools/layout.ts sync    [--root DIR]   write the generated parts of Cargo.toml, package.json, pyproject.toml, ...
//   bun tools/layout.ts check   [--root DIR]   fail on drift, uncovered paths, escaping or unpinned dependencies, stale old paths
//   bun tools/layout.ts adopt   [--root DIR]   write layout.toml [cargo] from the manifests as they are today
//   bun tools/layout.ts areas   [--root DIR]   print tracked paths grouped by top and second level, with their area (or none)
//   bun tools/layout.ts move    [--root DIR]   execute the area entries whose decision is "move" and not blocked (git mv, references rewritten), then sync
//   bun tools/layout.ts rustonly [--root DIR]  rewrite the baseline of non-Rust source files ([rustonly] in layout.toml)
//   bun tools/layout.ts doc     [--root DIR]   write the generated tables of docs/architecture/layout-2026.md (area decisions, move table)
//
// Why one file per repository and not one shared file: the dependency direction is fw <- aphrody <- ay, so a lower repo
// must never name a higher one; each repo's layout.toml names only itself and the repos below it, and travels with the repo.
// Why Bun and not a Cargo subcommand: aphrody and ay CI check out ../fw anyway for the path dependencies, a text generator
// needs no compile step, and the same script covers Bun workspaces, uv, Gradle, .NET and CMake, which cargo cannot.
import { $ } from "bun";
import { chmodSync, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, posix, resolve } from "node:path";

type Scalar = string | number | boolean | string[];
type Home = {
  repo?: string; path?: string; git?: string; rev?: string; tag?: string; branch?: string; version?: string; registry?: string;
  package?: string; "default-features"?: boolean; features?: string[]; optional?: boolean; why?: string; source?: string; crate?: string;
};
type Area = { path: string; decision: "keep" | "move" | "delete"; to?: string; why: string; blocked?: string };
type Moved = { from: string; to: string; why: string };
type Config = {
  repo: string;
  repos: Record<string, { layer: number; path: string; url: string }>;
  policy?: { pinned_git?: boolean; no_submodules?: boolean; no_vendor?: boolean };
  cargo?: { members?: string[]; crate?: Record<string, Home>; patch?: Home[]; package?: Record<string, Scalar> };
  bun?: { workspaces: string[]; exclude?: string[]; debt?: { path: string; why: string }[] };
  uv?: { members: string[]; exclude?: string[] };
  gradle?: { root: string; include: string[]; catalog?: string };
  dotnet?: { root: string; solution: string; projects: string[] };
  cmake?: { projects: string[] };
  area?: Area[];
  moved?: Moved[];
  rustonly?: { baseline: string; allow?: string[]; extensions?: string[] };
};

const args = Bun.argv.slice(2);
const command = args[0] ?? "check";
const flag = (name: string) => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined);
const root = resolve(flag("--root") ?? process.cwd());
const layoutPath = join(root, "layout.toml");
const read = (relative: string) => readFileSync(join(root, relative), "utf8");
const problems: string[] = [];
const fail = (message: string) => problems.push(message);

const tracked = (await $`git -C ${root} ls-files --cached --others --exclude-standard`.quiet().text()).split("\n").filter(Boolean);
const trackedSet = new Set(tracked);

// ---------------------------------------------------------------- TOML text helpers (generated blocks keep the rest of the file byte for byte)

const quote = (value: string) => JSON.stringify(value);
const render = (value: Scalar) => (Array.isArray(value) ? `[${value.map(quote).join(", ")}]` : typeof value === "string" ? quote(value) : String(value));
const FIELD_ORDER = ["workspace", "version", "path", "git", "rev", "tag", "branch", "registry", "package", "default-features", "features", "optional"];
const inline = (fields: Record<string, Scalar>) => {
  const keys = Object.keys(fields).filter((k) => fields[k] !== undefined).sort((a, b) => FIELD_ORDER.indexOf(a) - FIELD_ORDER.indexOf(b));
  if (keys.length === 1 && keys[0] === "version") return render(fields.version);
  return `{ ${keys.map((k) => `${k} = ${render(fields[k])}`).join(", ")} }`;
};
const isHeader = (line: string) => /^\s*\[/.test(line);
const headerOf = (line: string) => /^\s*\[([^\]]+)\]\s*(#.*)?$/.exec(line)?.[1].trim();
const depthDelta = (line: string) => {
  let depth = 0, quoteChar = "";
  for (const ch of line) {
    if (quoteChar) { if (ch === quoteChar) quoteChar = ""; continue; }
    if (ch === '"' || ch === "'") quoteChar = ch;
    else if (ch === "#") break;
    else if (ch === "[" || ch === "{") depth++;
    else if (ch === "]" || ch === "}") depth--;
  }
  return depth;
};
// Key lines of one table with the line range each value spans, so multi-line values are removed whole.
function keyRanges(lines: string[], from: number, to: number) {
  const out: { key: string; start: number; end: number }[] = [];
  for (let i = from; i < to; i++) {
    const m = /^([A-Za-z0-9_.-]+|"[^"]+")\s*=/.exec(lines[i]);
    if (!m) continue;
    let end = i, depth = depthDelta(lines[i]);
    while (depth > 0 && end + 1 < to) depth += depthDelta(lines[++end]);
    out.push({ key: m[1].replace(/"/g, ""), start: i, end });
    i = end;
  }
  return out;
}
const tableEnd = (lines: string[], header: number) => {
  let i = header + 1;
  while (i < lines.length && !isHeader(lines[i])) i++;
  return i;
};
const markers = (name: string) => [`# layout:begin ${name} (generated by tools/layout.ts from layout.toml; edit layout.toml)`, `# layout:end ${name}`];
function stripBlock(lines: string[], name: string) {
  const [begin, end] = markers(name);
  const b = lines.findIndex((l) => l.startsWith(`# layout:begin ${name}`));
  const e = lines.findIndex((l) => l.startsWith(`# layout:end ${name}`));
  if (b >= 0 && e > b) lines.splice(b, e - b + 1);
  return { begin, end, at: b };
}
// Comment lines directly above a line belong to it.
const commentStart = (lines: string[], index: number) => {
  let i = index;
  while (i > 0 && /^\s*#/.test(lines[i - 1]) && !lines[i - 1].startsWith("# layout:")) i--;
  return i;
};

// ---------------------------------------------------------------- configuration

if (!existsSync(layoutPath) && command !== "adopt" && command !== "areas") {
  console.error(`no layout.toml in ${root}`);
  process.exit(2);
}
const config: Config = existsSync(layoutPath) ? (Bun.TOML.parse(readFileSync(layoutPath, "utf8")) as unknown as Config) : ({ repo: "", repos: {} } as Config);
const self = config.repos?.[config.repo];

// ---------------------------------------------------------------- Cargo

type Member = { dir: string; name: string; version?: string; publish: boolean; manifest: any };
const globToRegExp = (glob: string) => new RegExp(`^${glob.split("**").map((part) => part.split("*").map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join("[^/]*")).join(".*")}$`);
function members(): Member[] {
  const cargo = config.cargo;
  if (!cargo?.members) return [];
  const patterns = cargo.members.map(globToRegExp);
  const rootManifest = Bun.TOML.parse(read("Cargo.toml")) as any;
  const excluded = ((rootManifest.workspace?.exclude as string[]) ?? []).map(globToRegExp);
  const out: Member[] = [];
  for (const file of tracked.filter((f) => f.endsWith("/Cargo.toml") && !f.startsWith("target/"))) {
    const dir = posix.dirname(file);
    if (!patterns.some((p) => p.test(dir)) || excluded.some((p) => p.test(dir))) continue;
    const manifest = Bun.TOML.parse(read(file)) as any;
    if (!manifest.package?.name) continue;
    const version = typeof manifest.package.version === "string" ? manifest.package.version : manifest.package.version?.workspace ? rootManifest.workspace?.package?.version : undefined;
    out.push({ dir, name: manifest.package.name, version, publish: manifest.package.publish !== false, manifest });
  }
  return out;
}
const repoPath = (repo: string) => {
  const entry = config.repos[repo];
  if (!entry) throw new Error(`unknown repo ${repo}`);
  return repo === config.repo ? "" : entry.path;
};
const homeDir = (home: Home) => (home.repo ? posix.join(repoPath(home.repo), home.path ?? "") : home.path ?? "");
function homeFields(name: string, home: Home, local: Map<string, Member>): Record<string, Scalar> {
  const fields: Record<string, Scalar> = {};
  const localAlias = home.package && !home.git && !home.repo && (home.version === undefined || home.version === "auto");
  const member = localAlias ? local.get(home.package!) : local.get(name);
  if (home.git) {
    fields.git = home.git;
    for (const k of ["rev", "tag", "branch"] as const) if (home[k]) fields[k] = home[k]!;
    if (home.version) fields.version = home.version;
  } else if (home.repo || home.path || member) {
    const dir = home.repo || home.path ? homeDir(home) : member!.dir;
    fields.path = dir;
    if (home.version === "auto") {
      const version = member?.version ?? readHomeVersion(dir);
      if (version) fields.version = version;
    } else if (home.version) fields.version = home.version;
  } else if (home.version) fields.version = home.version;
  if (home.registry) fields.registry = home.registry;
  if (home.package) fields.package = home.package;
  for (const k of ["default-features", "features", "optional"] as const) if (home[k] !== undefined) fields[k] = home[k] as Scalar;
  return fields;
}
function readHomeVersion(dir: string) {
  const file = join(root, dir, "Cargo.toml");
  if (!existsSync(file)) return undefined;
  const manifest = Bun.TOML.parse(readFileSync(file, "utf8")) as any;
  return typeof manifest.package?.version === "string" ? manifest.package.version : undefined;
}
const whyLines = (why?: string) => (why ? why.trim().split("\n").map((l) => `# ${l}`.trimEnd()) : []);

function cargoSync(text: string): string {
  const cargo = config.cargo!;
  const lines = text.split("\n");
  // members
  if (cargo.members) {
    const start = lines.findIndex((l) => /^members\s*=\s*\[/.test(l));
    if (start >= 0) {
      let end = start, depth = depthDelta(lines[start]);
      while (depth > 0) depth += depthDelta(lines[++end]);
      lines.splice(start, end - start + 1, "members = [", ...cargo.members.map((m) => `    ${quote(m)},`), "]");
    }
  }
  // dependencies
  const local = new Map(members().map((m) => [m.name, m]));
  // A local crate gets a workspace entry only when some member inherits it (cargo warns about unused workspace dependencies); a crate
  // that has an explicit [cargo.crate.*] entry always gets one.
  const used = new Set<string>();
  for (const member of local.values()) {
    const m = member.manifest;
    const tables: any[] = [m.dependencies, m["dev-dependencies"], m["build-dependencies"], ...Object.values<any>(m.target ?? {}).flatMap((x) => [x.dependencies, x["dev-dependencies"], x["build-dependencies"]])];
    for (const table of tables) for (const [key, value] of Object.entries<any>(table ?? {})) if (value && typeof value === "object" && value.workspace) used.add(key);
  }
  const names = new Set([...[...local.keys()].filter((n) => used.has(n)), ...Object.keys(cargo.crate ?? {})]);
  const generated: string[] = [];
  const generatedKeys = new Set<string>();
  for (const name of [...names].sort()) {
    const home = cargo.crate?.[name] ?? {};
    const fields = homeFields(name, home, local);
    if (Object.keys(fields).length === 0) continue;
    generatedKeys.add(name);
    generated.push(...whyLines(home.why), `${/^[A-Za-z0-9_-]+$/.test(name) ? name : quote(name)} = ${inline(fields)}`);
  }
  stripBlock(lines, "cargo.dependencies");
  stripBlock(lines, "cargo.patch");
  const header = lines.findIndex((l) => headerOf(l) === "workspace.dependencies");
  if (header < 0) throw new Error("no [workspace.dependencies] table");
  const end = tableEnd(lines, header);
  const drop = keyRanges(lines, header + 1, end).filter((r) => generatedKeys.has(r.key));
  for (const range of drop.sort((a, b) => b.start - a.start)) lines.splice(commentStart(lines, range.start), range.end - commentStart(lines, range.start) + 1);
  const newEnd = tableEnd(lines, header);
  let at = commentStart(lines, newEnd);
  while (at > header + 1 && lines[at - 1].trim() === "") at--;
  const [begin, finish] = markers("cargo.dependencies");
  lines.splice(at, 0, "", begin, ...generated, finish);
  // [workspace.package] keys the map owns (version stays with tools/version.ts)
  stripBlock(lines, "cargo.package");
  if (cargo.package && Object.keys(cargo.package).length) {
    const packageHeader = lines.findIndex((l) => headerOf(l) === "workspace.package");
    if (packageHeader < 0) throw new Error("no [workspace.package] table");
    const packageEnd = tableEnd(lines, packageHeader);
    for (const range of keyRanges(lines, packageHeader + 1, packageEnd).filter((r) => r.key in cargo.package!).sort((a, b) => b.start - a.start)) lines.splice(range.start, range.end - range.start + 1);
    let packageAt = tableEnd(lines, packageHeader);
    while (packageAt > packageHeader + 1 && lines[packageAt - 1].trim() === "") packageAt--;
    const [packageBegin, packageFinish] = markers("cargo.package");
    lines.splice(packageAt, 0, packageBegin, ...Object.entries(cargo.package).map(([k, v]) => `${k} = ${render(v)}`), packageFinish);
  }
  // patches
  const patches = cargo.patch ?? [];
  stripPatchSections(lines, patches.length > 0 ? markers("cargo.patch") : null, patches, local);
  return lines.join("\n").replace(/\n{3,}/g, "\n\n");
}
function stripPatchSections(lines: string[], block: string[] | null, patches: Home[], local: Map<string, Member>) {
  stripBlock(lines, "cargo.patch");
  let first = -1;
  for (let i = 0; i < lines.length; i++) {
    const header = headerOf(lines[i]);
    if (!header || !/^patch[.]/.test(header)) continue;
    const start = commentStart(lines, i);
    const end = tableEnd(lines, i);
    if (first < 0) first = start;
    lines.splice(start, end - start);
    i = start - 1;
  }
  if (!block) return;
  if (first < 0) first = lines.findIndex((l) => /^\[profile/.test(l));
  if (first < 0) first = lines.length;
  const groups = new Map<string, Home[]>();
  for (const patch of patches) groups.set(patch.source ?? "crates-io", [...(groups.get(patch.source ?? "crates-io") ?? []), patch]);
  const out = [block[0]];
  for (const [source, entries] of groups) {
    out.push(`[patch.${/^[A-Za-z0-9_-]+$/.test(source) ? source : quote(source)}]`);
    for (const entry of entries) {
      const fields = homeFields(entry.crate!, entry, local);
      delete fields.version;
      out.push(...whyLines(entry.why), `${entry.crate} = ${inline(fields)}`);
    }
    out.push("");
  }
  out.pop();
  out.push(block[1], "");
  lines.splice(first, 0, ...out);
}

type Violation = { file: string; key: string; message: string };
function memberViolations(): Violation[] {
  const cargo = config.cargo!;
  const local = new Map(members().map((m) => [m.name, m]));
  const byName = new Set([...local.keys(), ...Object.keys(cargo.crate ?? {})]);
  const out: Violation[] = [];
  for (const member of local.values()) {
    const m = member.manifest;
    const tables: any[] = [m.dependencies, m["dev-dependencies"], m["build-dependencies"],
      ...Object.values<any>(m.target ?? {}).flatMap((t) => [t.dependencies, t["dev-dependencies"], t["build-dependencies"]])];
    for (const table of tables) for (const [key, value] of Object.entries<any>(table ?? {})) {
      if (!value || typeof value !== "object") continue;
      const real = value.package ?? key;
      const file = `${member.dir}/Cargo.toml`;
      if (value.path) {
        const target = posix.normalize(posix.join(member.dir, value.path));
        if (target === member.dir) continue;
        if (target.startsWith("..")) out.push({ file, key, message: `${key} leaves the repository (path ${value.path}); declare it in layout.toml [cargo.crate.${key}] and use workspace = true` });
        else if (byName.has(real)) out.push({ file, key, message: `${key} names path ${value.path}; use ${key}.workspace = true` });
      } else if (value.git && byName.has(real)) {
        // A member may keep its own git source when the workspace entry points at another revision (one lane per origin).
        const home = cargo.crate?.[key];
        const sameSource = !home?.git || (home.git === value.git && home.rev === value.rev && home.tag === value.tag);
        if (sameSource) out.push({ file, key, message: `${key} names a git source; use workspace = true` });
      }
    }
  }
  return out;
}
function cargoCheck() {
  const cargo = config.cargo!;
  const crates = cargo.crate ?? {};
  const all = [...Object.entries(crates), ...(cargo.patch ?? []).map((p) => [p.crate!, p] as const)];
  const local = new Map(members().map((m) => [m.name, m]));
  for (const [name, home] of all) {
    if (home.repo) {
      const target = config.repos[home.repo];
      if (!target) { fail(`cargo.crate.${name}: unknown repo ${home.repo}`); continue; }
      if (target.layer > (self?.layer ?? 0)) fail(`cargo.crate.${name}: ${home.repo} (layer ${target.layer}) is above ${config.repo} (layer ${self?.layer}); direction is fw <- aphrody <- ay`);
      const manifest = join(root, homeDir(home), "Cargo.toml");
      if (!existsSync(manifest)) { fail(`cargo.crate.${name}: ${homeDir(home)}/Cargo.toml does not exist (is ${home.repo} checked out beside this repo?)`); continue; }
      const real = (Bun.TOML.parse(readFileSync(manifest, "utf8")) as any).package?.name;
      if (real !== (home.package ?? name)) fail(`cargo.crate.${name}: ${homeDir(home)} is package ${real}, the map says ${home.package ?? name}`);
    }
    if (config.policy?.pinned_git && home.git && !home.rev && !home.tag) fail(`cargo.crate.${name}: git dependency without rev or tag`);
    if (home.git && home.branch && !home.rev && !home.tag) fail(`cargo.crate.${name}: branch is not a pin`);
  }
  for (const v of memberViolations()) fail(`${v.file}: ${v.message}`);
}

// adopt: layout.toml [cargo] from the manifests as they are
function cargoAdopt(): string {
  const text = read("Cargo.toml");
  const manifest = Bun.TOML.parse(text) as any;
  const lines = text.split("\n");
  const globs = (manifest.workspace?.members as string[]) ?? [];
  config.cargo = { members: globs };
  const out: string[] = [];
  const siblings = Object.entries(config.repos).filter(([name]) => name !== config.repo);
  const asHome = (name: string, spec: any, why?: string): Home | null => {
    const home: Home = {};
    if (spec.path) {
      const normal = posix.normalize(spec.path);
      const sibling = siblings.find(([, r]) => normal.startsWith(`${posix.normalize(r.path)}/`));
      if (sibling) { home.repo = sibling[0]; home.path = normal.slice(posix.normalize(sibling[1].path).length + 1); }
      else home.path = normal;
    }
    for (const k of ["git", "rev", "tag", "branch", "registry", "package", "default-features", "features", "optional"] as const) if (spec[k] !== undefined) (home as any)[k] = spec[k];
    if (spec.version) home.version = spec.version;
    if (why) home.why = why;
    return home;
  };
  const commentsAbove = (line: number) => lines.slice(commentStart(lines, line), line).map((l) => l.replace(/^\s*#\s?/, "")).join("\n");
  const header = lines.findIndex((l) => headerOf(l) === "workspace.dependencies");
  const ranges = keyRanges(lines, header + 1, tableEnd(lines, header));
  const found = new Map(ranges.map((r) => [r.key, r]));
  const local = new Map(members().map((m) => [m.name, m]));
  const manifests = tracked.filter((f) => f.endsWith("/Cargo.toml")).map((f) => posix.dirname(f));
  const listed = new Set(globs);
  const excludedDirs = ((manifest.workspace?.exclude as string[]) ?? []).map(globToRegExp);
  const compressed: string[] = [];
  const covered = new Set<string>();
  for (const dir of new Set(globs.map((g) => posix.dirname(g)).filter((d) => d !== "."))) {
    const siblings = manifests.filter((m) => posix.dirname(m) === dir && !excludedDirs.some((p) => p.test(m)));
    const childDirs = new Set(tracked.filter((f) => f.startsWith(`${dir}/`)).map((f) => f.slice(dir.length + 1).split("/")[0]).filter((_, i, a) => true));
    const bare = [...childDirs].filter((c) => tracked.some((f) => f.startsWith(`${dir}/${c}/`)) && !manifests.includes(`${dir}/${c}`));
    if (bare.length === 0 && siblings.length > 1 && siblings.every((m) => listed.has(m))) { compressed.push(`${dir}/*`); siblings.forEach((m) => covered.add(m)); }
  }
  const finalMembers = [...compressed, ...globs.filter((g) => !covered.has(g))];
  out.push(`[cargo]`, "members = [", ...finalMembers.map((m) => `    ${quote(m)},`), "]", "");
  for (const [name, spec] of Object.entries<any>(manifest.workspace?.dependencies ?? {})) {
    if (typeof spec !== "object") continue;
    const localMember = local.get(name);
    const isLocalPath = spec.path && localMember && posix.normalize(spec.path) === localMember.dir;
    if (!(spec.path || spec.git || spec.package)) continue;
    const home = asHome(name, spec, commentsAbove(found.get(name)?.start ?? 0));
    if (!home) continue;
    if (isLocalPath) delete home.path;
    if (home.version && localMember?.version === home.version) home.version = "auto";
    if (isLocalPath && Object.keys(home).filter((k) => k !== "why").length === 0) continue;
    out.push(`[cargo.crate.${/^[A-Za-z0-9_-]+$/.test(name) ? name : quote(name)}]`, ...tomlFields(home), "");
  }
  const patches: Home[] = [];
  for (const [source, table] of Object.entries<any>(manifest.patch ?? {})) {
    const sectionHeader = lines.findIndex((l) => headerOf(l) === `patch.${source}` || headerOf(l) === `patch."${source}"`);
    const rs = keyRanges(lines, sectionHeader + 1, tableEnd(lines, sectionHeader));
    const index = new Map(rs.map((r) => [r.key, r]));
    for (const [name, spec] of Object.entries<any>(table)) {
      const home = asHome(name, spec, commentsAbove(index.get(name)?.start ?? 0)) ?? {};
      patches.push({ source, crate: name, ...home });
    }
    // The comment above the table header explains the whole section; keep it on its first entry.
    const lead = commentsAbove(sectionHeader);
    if (lead && patches.length) { const first = patches.find((p) => p.source === source); if (first && !first.why) first.why = lead; }
  }
  for (const patch of patches) out.push(`[[cargo.patch]]`, ...tomlFields(patch), "");
  return out.join("\n").replace(/\n+$/, "\n");
}
function tomlFields(home: Home) {
  const order = ["source", "crate", "repo", "path", "git", "rev", "tag", "branch", "registry", "version", "package", "default-features", "features", "optional", "why"];
  return order.filter((k) => (home as any)[k] !== undefined).map((k) => {
    const value = (home as any)[k];
    return k === "why" && String(value).includes("\n") ? `why = """\n${value}\n"""` : `${k} = ${render(value)}`;
  });
}

// ---------------------------------------------------------------- other ecosystems (member lists written into the native manifest)

const globFiles = (patterns: string[], suffix: string, excludes: string[] = []) => {
  const include = patterns.map(globToRegExp);
  const exclude = excludes.map(globToRegExp);
  return tracked.filter((f) => f.endsWith(`/${suffix}`) || f === suffix).map((f) => posix.dirname(f)).filter((d) => include.some((p) => p.test(d)) && !exclude.some((p) => p.test(d)));
};
function jsonSync(file: string, edit: (data: any) => void) {
  const text = read(file);
  const data = JSON.parse(text);
  edit(data);
  const indent = /^\{\n(\s+)"/.exec(text)?.[1] ?? "  ";
  return JSON.stringify(data, null, indent.length === 1 && indent === "\t" ? "\t" : indent.length) + (text.endsWith("\n") ? "\n" : "");
}
function ecosystems(): Record<string, string> {
  const out: Record<string, string> = {};
  if (config.bun) out["package.json"] = jsonSync("package.json", (data) => { data.workspaces = Array.isArray(data.workspaces) || !data.workspaces?.packages ? config.bun!.workspaces : { ...data.workspaces, packages: config.bun!.workspaces }; });
  if (config.uv) {
    const lines = read("pyproject.toml").split("\n");
    let header = lines.findIndex((l) => headerOf(l) === "tool.uv.workspace");
    const body = [`members = [${config.uv.members.map(quote).join(", ")}]`, ...(config.uv.exclude?.length ? [`exclude = [${config.uv.exclude.map(quote).join(", ")}]`] : [])];
    if (header < 0) lines.push("", "[tool.uv.workspace]", ...body, "");
    else lines.splice(header + 1, tableEnd(lines, header) - header - 1, ...body, "");
    out["pyproject.toml"] = lines.join("\n").replace(/\n{3,}/g, "\n\n");
  }
  if (config.gradle) {
    const file = posix.join(config.gradle.root, "settings.gradle.kts");
    const lines = read(file).split("\n");
    // Drops every existing include(...) statement, one line or many, and the previous generated block.
    const kept: string[] = [];
    for (let i = 0; i < lines.length; i++) {
      if (/^include\(/.test(lines[i])) {
        let depth = (lines[i].match(/\(/g) ?? []).length - (lines[i].match(/\)/g) ?? []).length;
        while (depth > 0 && i + 1 < lines.length) { i++; depth += (lines[i].match(/\(/g) ?? []).length - (lines[i].match(/\)/g) ?? []).length; }
        continue;
      }
      kept.push(lines[i]);
    }
    const body = kept.filter((l) => !l.startsWith("// layout:"));
    body.push("// layout:begin gradle (generated by tools/layout.ts from layout.toml; edit layout.toml)", `include(${config.gradle.include.map((p) => quote(`:${p}`)).join(", ")})`, "// layout:end gradle");
    out[file] = body.join("\n").replace(/\n{3,}/g, "\n\n").replace(/\n*$/, "\n");
  }
  if (config.dotnet) {
    const files = tracked.filter((f) => f.endsWith(".csproj") && config.dotnet!.projects.some((p) => globToRegExp(p).test(posix.dirname(f)))).sort();
    out[posix.join(config.dotnet.root, config.dotnet.solution)] = `<Solution>\n${files.map((f) => `  <Project Path="${posix.relative(config.dotnet!.root, f)}" />`).join("\n")}\n</Solution>\n`;
  }
  return out;
}
function ecosystemCheck() {
  if (config.cmake) for (const project of config.cmake.projects) if (!trackedSet.has(posix.join(project, "CMakePresets.json"))) fail(`cmake: ${project} has no CMakePresets.json (configure, build and test presets are the entry point)`);
  if (config.dotnet) {
    const root = config.dotnet.root;
    for (const file of ["Directory.Build.props", "Directory.Packages.props"]) if (!trackedSet.has(posix.join(root, file))) fail(`dotnet: ${posix.join(root, file)} is missing`);
    if (trackedSet.has(posix.join(root, "Directory.Packages.props")) && !read(posix.join(root, "Directory.Packages.props")).includes("<ManagePackageVersionsCentrally>true")) fail("dotnet: Directory.Packages.props does not enable central package management");
    for (const file of tracked.filter((f) => f.endsWith(".csproj"))) if (/<PackageReference[^>]*\sVersion=/.test(read(file))) fail(`dotnet: ${file} pins a package version; versions live in Directory.Packages.props`);
  }
  if (config.gradle?.catalog && !trackedSet.has(config.gradle.catalog)) fail(`gradle: version catalog ${config.gradle.catalog} is missing`);
  const checks: [string, string[] | undefined, string, string[] | undefined][] = [
    ["bun", config.bun?.workspaces, "package.json", config.bun?.exclude],
    ["uv", config.uv?.members, "pyproject.toml", config.uv?.exclude],
  ];
  for (const [name, patterns, manifest, exclude] of checks) {
    if (!patterns) continue;
    const declared = new Set(globFiles(patterns, manifest, exclude));
    const rootDir = ".";
    const covered = [...patterns, ...(exclude ?? [])].map(globToRegExp);
    const onDisk = tracked.filter((f) => f.endsWith(`/${manifest}`) && !f.includes("node_modules/")).map((f) => posix.dirname(f));
    for (const dir of onDisk) if (dir !== rootDir && !covered.some((p) => p.test(dir)) && !declared.has(dir)) fail(`${name}: ${dir}/${manifest} is neither a workspace member nor excluded in layout.toml [${name}]`);
  }
  if (config.bun) {
    const debt = new Map((config.bun.debt ?? []).map((d) => [d.path, d.why]));
    const debtSeen = new Set<string>();
    const names = new Map<string, string>();
    for (const dir of globFiles(config.bun.workspaces, "package.json", config.bun.exclude)) names.set(JSON.parse(read(`${dir}/package.json`)).name, dir);
    for (const dir of globFiles(config.bun.workspaces, "package.json", config.bun.exclude)) {
      const data = JSON.parse(read(`${dir}/package.json`));
      for (const field of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) for (const [dep, spec] of Object.entries<string>(data[field] ?? {})) {
        if (names.has(dep) && !/^(workspace:|catalog:)/.test(spec)) fail(`${dir}/package.json: ${dep} is a workspace package, use "workspace:*" (found ${spec})`);
        if (/^(file|link):/.test(spec) && spec.includes("..")) {
          const outside = posix.normalize(posix.join(dir, spec.replace(/^(file|link):/, ""))).startsWith("..");
          if (!outside) fail(`${dir}/package.json: ${dep} points at a path (${spec}); use workspace:*`);
          else if (debt.has(dir)) debtSeen.add(dir);
          else fail(`${dir}/package.json: ${dep} points into another repository (${spec}); a lower repo must not depend on a higher one, declare the package as a workspace outside the root or add a [[bun.debt]] entry with the reason`);
        }
      }
    }
    for (const path of debt.keys()) if (!debtSeen.has(path)) fail(`bun.debt ${path}: no cross-repository link left, remove the entry`);
  }
}

// ---------------------------------------------------------------- coverage and stale references

function areaOf(path: string) {
  let best: Area | undefined;
  for (const area of config.area ?? []) {
    const hit = area.path === path || path.startsWith(`${area.path}/`) || (area.path.includes("*") && globToRegExp(area.path).test(path));
    if (hit && (!best || area.path.length > best.path.length)) best = area;
  }
  return best;
}
function coverageCheck() {
  if (!config.area) return;
  const missing = tracked.filter((f) => !areaOf(f));
  if (missing.length) {
    const groups = new Map<string, number>();
    for (const f of missing) { const key = f.split("/").slice(0, f.includes("/") ? 2 : 1).join("/"); groups.set(key, (groups.get(key) ?? 0) + 1); }
    fail(`${missing.length} tracked paths are not covered by an [[area]] in layout.toml: ${[...groups].slice(0, 25).map(([k, n]) => `${k} (${n})`).join(", ")}${groups.size > 25 ? ", ..." : ""}`);
  }
  for (const area of config.area) {
    if (area.decision === "delete" && tracked.some((f) => f === area.path || f.startsWith(`${area.path}/`))) fail(`area ${area.path}: decision is delete but paths are still tracked`);
    if (area.decision === "move" && !area.to) fail(`area ${area.path}: decision move needs "to"`);
    if (!area.why) fail(`area ${area.path}: why is empty`);
  }
}
async function staleReferences() {
  for (const moved of config.moved ?? []) {
    const hits = (await $`git -C ${root} grep -nIF -e ${moved.from} -- . ${":(exclude)layout.toml"} ${":(exclude)docs/architecture/layout-2026.md"}`.quiet().nothrow().text()).split("\n").filter(Boolean);
    const boundary = new RegExp(`(^|[^\\w./-])/?${moved.from.replace(/[.+?^${}()|[\]\\]/g, "\\$&")}(/|$|[^\\w-])`);
    for (const hit of hits) if (boundary.test(hit.replace(/^[^:]+:\d+:/, ""))) fail(`stale reference to moved path ${moved.from} (now ${moved.to}): ${hit.slice(0, 160)}`);
  }
}
const NON_RUST = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "py", "java"];
function nonRustSources() {
  const cfg = config.rustonly!;
  const extensions = new Set(cfg.extensions ?? NON_RUST);
  const allow = (cfg.allow ?? []).map(globToRegExp);
  return tracked.filter((f) => extensions.has(f.slice(f.lastIndexOf(".") + 1)) && !allow.some((p) => p.test(f))).sort();
}
// Pure Rust is the direction: a non-Rust source file must be in the baseline (what exists today, shrinking as ports land) or match a
// deliberate allow entry (generated kernel types, vendored third-party scripts); anything else is new and fails.
function rustOnlyCheck() {
  if (!config.rustonly) return;
  const baseline = new Set(existsSync(join(root, config.rustonly.baseline)) ? read(config.rustonly.baseline).split("\n").filter(Boolean) : []);
  const added = nonRustSources().filter((f) => !baseline.has(f));
  if (added.length) fail(`${added.length} new non-Rust source files outside the baseline and the deliberate list (write it in Rust, or add a deliberate [rustonly] allow entry with the reason): ${added.slice(0, 8).join(", ")}${added.length > 8 ? ", ..." : ""}`);
}
function policyCheck() {
  if (config.policy?.no_submodules && trackedSet.has(".gitmodules")) fail(".gitmodules is tracked; this repository forbids submodules (pinned git dependencies only)");
  if (config.policy?.no_vendor && tracked.some((f) => f.startsWith("vendor/"))) fail("vendor/ is tracked; this repository forbids a vendor directory");
}

// ---------------------------------------------------------------- commands

function writeAll(changes: Record<string, string>) {
  for (const [file, content] of Object.entries(changes)) writeFileSync(join(root, file), content);
}
function generated(): Record<string, string> {
  const out: Record<string, string> = { ...ecosystems() };
  if (config.cargo) out["Cargo.toml"] = cargoSync(read("Cargo.toml"));
  return out;
}

if (command === "adopt") {
  const header = [`repo = ${quote(config.repo || flag("--repo")!)}`];
  console.error("adopt needs layout.toml with repo and repos filled in; it prints the [cargo] part");
  if (!config.repo) { console.error("set repo and [repos.*] first"); process.exit(2); }
  void header;
  console.log(cargoAdopt());
} else if (command === "areas") {
  const groups = new Map<string, { files: number; area?: string }>();
  for (const f of tracked) {
    const parts = f.split("/");
    const key = parts.length === 1 ? f : parts.length === 2 ? `${parts[0]}/${parts[1]}` : `${parts[0]}/${parts[1]}/`;
    const g = groups.get(key) ?? { files: 0, area: areaOf(f)?.path };
    g.files++;
    groups.set(key, g);
  }
  for (const [key, g] of [...groups].sort()) console.log(`${String(g.files).padStart(6)}  ${key}  ${g.area ?? "-- uncovered"}`);
} else if (command === "sync" || command === "check") {
  const changes = generated();
  const drift: string[] = [];
  for (const [file, content] of Object.entries(changes)) {
    const current = existsSync(join(root, file)) ? read(file) : "";
    if (current !== content) drift.push(file);
  }
  if (command === "sync") { writeAll(changes); console.log(JSON.stringify({ wrote: drift })); }
  else {
    for (const file of drift) fail(`${file} is not what layout.toml generates (run: bun tools/layout.ts sync)`);
    if (config.cargo) cargoCheck();
    ecosystemCheck();
    coverageCheck();
    policyCheck();
    rustOnlyCheck();
    await staleReferences();
  }
} else if (command === "fix") {
  // Moves every dependency on a mapped crate behind workspace = true. Default features keep their meaning: when any user
  // switches them off, the workspace entry switches them off and the users that want them say default-features = true.
  const generatedWorkspace = (Bun.TOML.parse(generated()["Cargo.toml"]) as any).workspace.dependencies as Record<string, any>;
  const violations = memberViolations();
  const keys = new Set(violations.map((v) => v.key));
  type Usage = { file: string; line: number; key: string; violating: boolean };
  const files = new Map<string, string[]>();
  const manual: string[] = [];
  for (const member of members()) files.set(`${member.dir}/Cargo.toml`, read(`${member.dir}/Cargo.toml`).split("\n"));
  const usages: Usage[] = [];
  for (const [file, lines] of files) {
    let inDependencies = false;
    lines.forEach((text, line) => {
      const header = headerOf(text);
      if (header !== undefined) { inDependencies = /(^|\.)(dev-|build-)?dependencies(\.|$)/.test(header); const m = /(?:^|\.)(?:dev-|build-)?dependencies\.([^.]+)$/.exec(header); if (m && keys.has(m[1])) manual.push(`${file}: [${header}] is a multi-line table; edit by hand`); return; }
      if (!inDependencies) return;
      const m = /^\s*([A-Za-z0-9_-]+)(?:\.workspace\s*=\s*true|\s*=\s*\{.*\})\s*(#.*)?$/.exec(text);
      if (m && keys.has(m[1])) usages.push({ file, line, key: m[1], violating: violations.some((v) => v.file === file && v.key === m[1]) && !/workspace\s*=\s*true/.test(text) });
    });
  }
  const fieldsOf = (text: string) => {
    const body = /\{(.*)\}/.exec(text)?.[1] ?? "";
    const parts: string[] = [];
    let depth = 0, quoteChar = "", current = "";
    for (const ch of body) {
      if (quoteChar) { current += ch; if (ch === quoteChar) quoteChar = ""; continue; }
      if (ch === '"' || ch === "'") { quoteChar = ch; current += ch; }
      else if (ch === "[") { depth++; current += ch; }
      else if (ch === "]") { depth--; current += ch; }
      else if (ch === "," && depth === 0) { parts.push(current.trim()); current = ""; }
      else current += ch;
    }
    if (current.trim()) parts.push(current.trim());
    return parts;
  };
  const switchOff = new Set<string>();
  for (const usage of usages) {
    const text = files.get(usage.file)![usage.line];
    if (usage.violating && fieldsOf(text).some((p) => /^default-features\s*=\s*false/.test(p)) && generatedWorkspace[usage.key]?.["default-features"] !== false) switchOff.add(usage.key);
  }
  for (const usage of usages) {
    const lines = files.get(usage.file)!;
    const text = lines[usage.line];
    const parts = fieldsOf(text).filter((p) => !/^(path|version|git|rev|tag|branch|workspace)\s*=/.test(p));
    const off = parts.findIndex((p) => /^default-features\s*=\s*false/.test(p));
    const on = parts.findIndex((p) => /^default-features\s*=\s*true/.test(p));
    const workspaceOff = generatedWorkspace[usage.key]?.["default-features"] === false || switchOff.has(usage.key);
    if (off >= 0) parts.splice(off, 1);
    else if (on < 0 && switchOff.has(usage.key)) parts.push("default-features = true");
    if (!workspaceOff && on >= 0) parts.splice(on, 1);
    const indent = /^\s*/.exec(text)![0];
    const comment = /\}\s*(#.*)$|workspace\s*=\s*true\s*(#.*)$/.exec(text);
    const tail = comment ? ` ${comment[1] ?? comment[2]}` : "";
    lines[usage.line] = parts.length ? `${indent}${usage.key} = { workspace = true, ${parts.join(", ")} }${tail}` : `${indent}${usage.key}.workspace = true${tail}`;
  }
  for (const [file, lines] of files) writeFileSync(join(root, file), lines.join("\n"));
  if (switchOff.size) {
    let text = readFileSync(layoutPath, "utf8");
    for (const key of switchOff) {
      const header = `[cargo.crate.${key}]`;
      if (text.includes(`${header}\n`)) text = text.replace(`${header}\n`, () => `${header}\ndefault-features = false\n`);
      else {
        const patchAt = text.indexOf("[[cargo.patch]]");
        const block = `${header}\ndefault-features = false\n\n`;
        text = patchAt >= 0 ? text.slice(0, patchAt) + block + text.slice(patchAt) : `${text.replace(/\n*$/, "\n")}\n${block}`;
      }
    }
    writeFileSync(layoutPath, text);
  }
  for (const message of manual) fail(message);
  console.log(JSON.stringify({ rewritten: usages.length, defaultFeaturesOff: [...switchOff] }));
} else if (command === "move") {
  // Executes pending moves with git mv, rewrites every repo-root-relative reference to the old path in tracked text
  // files, and records the move in [[moved]] so `check` keeps failing on any later reference to the old path.
  const pending = (config.area ?? []).filter((a) => a.decision === "move" && a.to && !a.blocked);
  let text = readFileSync(layoutPath, "utf8");
  const recorded: string[] = [];
  for (const area of pending) {
    if (!tracked.some((f) => f === area.path || f.startsWith(`${area.path}/`))) continue;
    mkdirSync(dirname(join(root, area.to!)), { recursive: true });
    await $`git -C ${root} mv ${area.path} ${area.to!}`;
    console.log(`${area.path} -> ${area.to}`);
    const escaped = area.path.replace(/[.+?^${}()|[\]\\]/g, "\\$&");
    const boundary = new RegExp(`(^|[^\\w./-])(/?)${escaped}(?=/|$|[^\\w-])`, "g");
    const files = (await $`git -C ${root} ls-files`.quiet().text()).split("\n").filter((f) => f && !f.endsWith(".lock") && f !== "layout.toml" && f !== "docs/architecture/layout-2026.md");
    for (const file of files) {
      const full = join(root, file);
      if (!existsSync(full) || !statSync(full).isFile() || statSync(full).size > 2_000_000) continue;
      const buffer = readFileSync(full);
      if (buffer.includes(0)) continue;
      const original = buffer.toString("utf8");
      const rewritten = original.replace(boundary, (_m, lead: string, slash: string) => `${lead}${slash}${area.to}`);
      if (rewritten !== original) { const mode = statSync(full).mode & 0o777; writeFileSync(full, rewritten); chmodSync(full, mode); }
    }
    recorded.push(`\n[[moved]]\nfrom = ${quote(area.path)}\nto = ${quote(area.to!)}\nwhy = ${quote(area.why)}\n`);
    text = text.replace(new RegExp(`\\[\\[area\\]\\]\\npath = "${escaped}"\\ndecision = "move"\\nto = "[^"]*"\\n(blocked = "[^"]*"\\n)?why = "[^"]*"\\n\\n?`), () => "");
    const hostArea = `[[area]]\npath = ${quote(area.to!)}\ndecision = "keep"\nwhy = ${quote(area.why)}\n\n`;
    text = text.replace(/\n*$/, "\n\n") + hostArea;
  }
  writeFileSync(layoutPath, text.replace(/\n*$/, "\n") + recorded.join(""));
} else if (command === "rustonly") {
  if (!config.rustonly) { console.error("no [rustonly] in layout.toml"); process.exit(2); }
  const files = nonRustSources();
  mkdirSync(dirname(join(root, config.rustonly.baseline)), { recursive: true });
  writeFileSync(join(root, config.rustonly.baseline), files.join("\n") + "\n");
  console.log(JSON.stringify({ baseline: config.rustonly.baseline, files: files.length }));
} else if (command === "doc") {
  const file = "docs/architecture/layout-2026.md";
  const counts = new Map<Area | undefined, number>();
  for (const f of tracked) { const a = areaOf(f); counts.set(a, (counts.get(a) ?? 0) + 1); }
  const rows = [...(config.area ?? [])].sort((x, y) => x.path.localeCompare(y.path)).map((a) => `| \`${a.path}\` | ${counts.get(a) ?? 0} | ${a.decision}${a.to ? ` -> \`${a.to}\`` : ""}${a.blocked ? " (blocked)" : ""} | ${a.why.replace(/\|/g, "\\|")}${a.blocked ? ` Blocked: ${a.blocked.replace(/\|/g, "\\|")}` : ""} |`);
  const moves = (config.moved ?? []).map((m) => `| \`${m.from}\` | \`${m.to}\` | ${m.why.replace(/\|/g, "\\|")} | done |`);
  for (const a of config.area ?? []) if (a.decision === "move") moves.push(`| \`${a.path}\` | \`${a.to}\` | ${a.why.replace(/\|/g, "\\|")} | ${a.blocked ? `blocked: ${a.blocked.replace(/\|/g, "\\|")}` : "pending"} |`);
  const body = [`<!-- layout:begin generated (bun tools/layout.ts doc, from layout.toml and git ls-files of ${tracked.length} paths) -->`,
    "", "### Every tracked path, by area", "", "| area | files | decision | reason |", "| --- | ---: | --- | --- |", ...rows, "",
    "### Move table (old path -> new path)", "", "| old path | new path | reason | status |", "| --- | --- | --- | --- |", ...(moves.length ? moves : ["| (none) | | | |"]), "", "<!-- layout:end generated -->"].join("\n");
  const path = join(root, file);
  const current = existsSync(path) ? readFileSync(path, "utf8") : `# Layout 2026: ${config.repo}\n\n`;
  const b = current.indexOf("<!-- layout:begin generated"), e = current.indexOf("<!-- layout:end generated -->");
  const next = b >= 0 && e > b ? current.slice(0, b) + body + current.slice(e + "<!-- layout:end generated -->".length) : `${current.replace(/\n*$/, "\n\n")}${body}\n`;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, next);
  console.log(JSON.stringify({ wrote: file, areas: rows.length, moves: moves.length }));
} else { console.error(`unknown command ${command}`); process.exit(2); }

if (problems.length) {
  console.error(problems.map((p) => `layout: ${p}`).join("\n"));
  process.exit(1);
}
if (command === "check") console.log(JSON.stringify({ ok: true, repo: config.repo, trackedPaths: tracked.length }));
