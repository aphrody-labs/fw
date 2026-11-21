#!/usr/bin/env bun
// Phase 1 of the monorepo: one Cargo workspace for fw, aphrody and ay.
//   bun tools/monorepo/unify.ts [--root /srv/xbuild/merge/fw] [--dry]
// Input: a tree where aphrody/ and ay/ are imported (tools/monorepo/import.ts) and still own a root Cargo.toml.
// What it does, from the manifests alone (nothing about a crate is hardcoded here):
//   1. members: fw's list, then every aphrody and ay member prefixed with its directory; excludes are prefixed the same way;
//   2. [workspace.dependencies]: hoisted when the origins agree; where specs differ, the first origin (fw, aphrody, ay) is hoisted at the
//      highest version of the group and every other origin's members get their own spec spelled out, features merged, version unified;
//   3. [workspace.package] and [workspace.lints] stay fw's: the other origins' members get literal package fields and inline lints;
//   4. [patch.*] and [profile.*] are unions (the first origin wins a clash, reported);
//   5. aphrody/ay root manifests and lock files are removed, .cargo/config.toml and rust-toolchain.toml gain the other origins' settings.
// Re-runnable on a fresh import. Prints a report of every decision.
import { existsSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";

type Table = Record<string, any>;
const argv = Bun.argv.slice(2);
const flag = (name: string, fallback: string) => (argv.includes(name) ? (argv[argv.indexOf(name) + 1] ?? fallback) : fallback);
const root = flag("--root", `${process.env.WORK ?? "/srv/xbuild"}/merge/fw`);
const dry = argv.includes("--dry");
const ORIGINS = ["fw", "aphrody", "ay"] as const;
type Origin = (typeof ORIGINS)[number];
const dirOf = (o: Origin) => (o === "fw" ? "" : `${o}/`);
const read = (p: string) => readFileSync(join(root, p), "utf8");
const parse = (p: string): Table => Bun.TOML.parse(read(p)) as Table;
const manifests: Record<Origin, Table> = { fw: parse("Cargo.toml"), aphrody: parse("aphrody/Cargo.toml"), ay: parse("ay/Cargo.toml") };
const report: string[] = [];
const note = (line: string) => report.push(line);

// --- TOML emission (the manifests are parsed with Bun.TOML, which cannot write) ---
const bare = /^[A-Za-z0-9_-]+$/;
const key = (k: string) => (bare.test(k) ? k : JSON.stringify(k));
const inline = (v: any): string =>
  Array.isArray(v)
    ? `[${v.map(inline).join(", ")}]`
    : v !== null && typeof v === "object"
      ? `{ ${Object.entries(v).map(([k, x]) => `${key(k)} = ${inline(x)}`).join(", ")} }`
      : JSON.stringify(v);
const isTable = (v: any) => v !== null && typeof v === "object" && !Array.isArray(v);
function emit(path: string[], table: Table, inlineChildren: boolean): string {
  const scalars = Object.entries(table).filter(([, v]) => !isTable(v) || inlineChildren);
  const children = Object.entries(table).filter(([, v]) => isTable(v) && !inlineChildren);
  const lines: string[] = [];
  if (scalars.length || !children.length) lines.push(`[${path.map(key).join(".")}]`, ...scalars.map(([k, v]) => `${key(k)} = ${inline(v)}`), "");
  for (const [k, v] of children) lines.push(emit([...path, k], v, path.length === 1 && (path[0] === "patch") ? true : false));
  return lines.join("\n");
}

// --- dependency specs ---
const normalize = (spec: any): Table => (typeof spec === "string" ? { version: spec } : spec);
const equal = (a: any, b: any) => JSON.stringify(sortKeys(a)) === JSON.stringify(sortKeys(b));
function sortKeys(v: any): any {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (isTable(v)) return Object.fromEntries(Object.keys(v).sort().map((k) => [k, sortKeys(v[k])]));
  return v;
}
const versionKey = (req: string) => (req.match(/\d+(\.\d+)*/)?.[0] ?? "0").split(".").map(Number);
const newer = (a: string, b: string) => {
  const [x, y] = [versionKey(a), versionKey(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  return false;
};
/** Resolves `..` segments and maps the pre-monorepo sibling checkout `fw/` onto the root. */
const inTree = (path: string) => {
  const out: string[] = [];
  for (const part of path.split("/")) if (part === "..") out.pop(); else if (part !== "." && part !== "") out.push(part);
  return (out[0] === "fw" ? out.slice(1) : out).join("/");
};
/** The semver lane a requirement lives in: major, or 0.minor, or 0.0.patch. */
const lane = (req: string) => {
  const [major = 0, minor = 0, patch = 0] = versionKey(req);
  return major > 0 ? `${major}` : minor > 0 ? `0.${minor}` : `0.0.${patch}`;
};
const prefixPath = (origin: Origin, spec: Table) => (spec.path && origin !== "fw" ? { ...spec, path: inTree(`${dirOf(origin)}${spec.path}`) } : spec);

const specs = new Map<string, Partial<Record<Origin, Table>>>();
for (const o of ORIGINS)
  for (const [name, spec] of Object.entries<any>(manifests[o].workspace?.dependencies ?? {}))
    (specs.get(name) ?? specs.set(name, {}).get(name)!)[o] = prefixPath(o, normalize(spec));

const hoisted: Table = {};
/** origin -> dependency name -> the spec that origin's members must spell out. */
const materialize: Record<Origin, Map<string, Table>> = { fw: new Map(), aphrody: new Map(), ay: new Map() };
let conflicts = 0;
for (const [name, byOrigin] of [...specs].sort(([a], [b]) => a.localeCompare(b))) {
  const defined = ORIGINS.filter((o) => byOrigin[o]);
  const first = byOrigin[defined[0]!]!;
  const versions = defined.map((o) => byOrigin[o]!.version).filter(Boolean) as string[];
  const top = versions.reduce((a, b) => (newer(b, a) ? b : a), versions[0] ?? "");
  const sameLane = new Set(versions.map(lane)).size <= 1;
  const agree = defined.every((o) => equal(byOrigin[o], first));
  // A git revision is newer than any release, so where one origin pins git the others follow it (their features stay).
  const gitSource = defined.map((o) => byOrigin[o]!).find((s) => s.git);
  const withVersion = (s: Table) => {
    if (gitSource && !s.path && !s.git) {
      const { version: _v, git: _g, rev: _r, tag: _t, branch: _b, ...rest } = s;
      const { git, rev, tag, branch } = gitSource;
      return { ...rest, git, ...(rev ? { rev } : {}), ...(tag ? { tag } : {}), ...(branch ? { branch } : {}) };
    }
    // across lanes the API differs: every origin keeps its own requirement and the lanes stay apart until the code moves
    return s.version && top && sameLane ? { ...s, version: top } : s;
  };
  hoisted[name] = agree ? first : withVersion(first);
  if (!agree) {
    conflicts++;
    note(`dependency ${name}: ${defined.map((o) => `${o} ${JSON.stringify(byOrigin[o])}`).join(" | ")} -> hoisted ${defined[0]}'s spec at ${sameLane ? top || "(no version)" : "its own version (lanes differ)"}`);
    for (const o of defined.slice(1)) if (!equal(withVersion(byOrigin[o]!), hoisted[name])) materialize[o].set(name, withVersion(byOrigin[o]!));
  }
}

// Decisions the manifests cannot express (feature sets that changed with a major version, a source to prefer) live in unify.toml:
//   [spec.<name>] hoisted = { ... } replaces the root spec; <origin> = { ... } is the exact spec that origin's members get.
const overridesFile = join(root, "tools/monorepo/unify.toml");
const overrides = (existsSync(overridesFile) ? Bun.TOML.parse(readFileSync(overridesFile, "utf8")) : {}) as { spec?: Record<string, Partial<Record<Origin | "hoisted", Table>>> };
for (const [name, per] of Object.entries(overrides.spec ?? {})) {
  if (per.hoisted) hoisted[name] = per.hoisted;
  for (const o of ORIGINS) {
    if (!per[o]) continue;
    if (equal(per[o], hoisted[name])) materialize[o].delete(name);
    else materialize[o].set(name, per[o]!);
  }
  note(`override for ${name}: ${Object.keys(per).join(", ")}`);
}

// --- members ---
const expand = (pattern: string): string[] => {
  if (!pattern.includes("*")) return [pattern];
  const base = pattern.slice(0, pattern.indexOf("*"));
  return readdirSync(join(root, base)).filter((d) => statSync(join(root, base, d)).isDirectory() && existsSync(join(root, base, d, "Cargo.toml"))).map((d) => `${base}${d}`);
};
const members: string[] = [...(manifests.fw.workspace.members ?? [])];
const excludes: string[] = [...(manifests.fw.workspace.exclude ?? []).filter((e: string) => !ORIGINS.includes(e as Origin))];
for (const o of ["aphrody", "ay"] as const) {
  for (const m of manifests[o].workspace.members ?? []) for (const p of expand(`${dirOf(o)}${m}`)) members.push(p);
  for (const e of manifests[o].workspace.exclude ?? []) excludes.push(`${dirOf(o)}${e}`);
}
const memberSet = new Set(members.filter((m) => !excludes.some((e) => e.endsWith("*") ? m.startsWith(e.slice(0, -1)) : m === e)));

// Members that cannot share the graph yet (unify.toml [[exclude]] path/why); they keep their directory and leave the workspace.
for (const e of ((overrides as any).exclude ?? []) as { path: string; why?: string }[]) {
  if (memberSet.delete(e.path)) {
    excludes.push(e.path);
    note(`excluded ${e.path}: ${e.why ?? ""}`);
  }
}

// Package names are unique per workspace: the first origin keeps a name, a later member with the same name leaves the workspace (reported; renames are a separate lot).
const seen = new Map<string, string>();
for (const m of [...memberSet]) {
  const name = (Bun.TOML.parse(read(join(m, "Cargo.toml"))) as Table).package?.name as string | undefined;
  if (!name) continue;
  if (seen.has(name)) {
    memberSet.delete(m);
    excludes.push(m);
    note(`package name ${name} is taken by ${seen.get(name)}: ${m} is excluded from the workspace`);
  } else seen.set(name, m);
}

// --- per-member rewrites ---
const PACKAGE_KEYS = ["version", "edition", "rust-version", "license", "repository", "homepage", "readme", "publish", "authors", "keywords", "categories", "description", "documentation"];
const lintsText = (lints: Table) => {
  const out: string[] = [];
  for (const [group, rules] of Object.entries<any>(lints)) {
    out.push(`[lints.${group}]`);
    for (const [rule, level] of Object.entries(rules)) out.push(`${key(rule)} = ${inline(level)}`);
    out.push("");
  }
  return out.join("\n");
};
let packageRewrites = 0;
let dependencyRewrites = 0;
let lintRewrites = 0;
const unhandled: string[] = [];
function rewriteMember(origin: Origin, member: string) {
  const file = join(member, "Cargo.toml");
  if (!existsSync(join(root, file))) return;
  let text = read(file);
  const before = text;
  // path dependencies written for sibling checkouts (../../../fw/crates/x) point at the in-repo location
  text = text.replace(/path = "((?:\.\.\/)+)fw\/(crates|native|os)\//g, 'path = "$1$2/');
  const base = manifests[origin].workspace.package ?? {};
  const rootPackage = manifests.fw.workspace.package ?? {};
  for (const k of PACKAGE_KEYS) {
    if (equal(base[k], rootPackage[k]) && base[k] !== undefined) continue;
    const literal = base[k];
    if (literal === undefined) continue;
    text = text.replace(new RegExp(`^(\\s*)${k}\\.workspace\\s*=\\s*true\\s*$`, "gm"), (_, lead) => ((packageRewrites++), `${lead}${k} = ${inline(literal)}`));
    text = text.replace(new RegExp(`^(\\s*)${k}\\s*=\\s*\\{\\s*workspace\\s*=\\s*true\\s*\\}\\s*$`, "gm"), (_, lead) => ((packageRewrites++), `${lead}${k} = ${inline(literal)}`));
  }
  if (origin !== "fw" && manifests[origin].workspace.lints)
    text = text.replace(/^\[lints\]\s*\r?\nworkspace\s*=\s*true\s*$/m, () => ((lintRewrites++), lintsText(manifests[origin].workspace.lints).trimEnd()));
  for (const [name, original] of materialize[origin]) {
    const spec = original.path ? { ...original, path: relative(join(root, member), join(root, original.path)).replaceAll("\\", "/") } : original;
    const esc = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    text = text.replace(new RegExp(`^(\\s*)${esc}\\.workspace\\s*=\\s*true\\s*$`, "gm"), (_, lead) => ((dependencyRewrites++), `${lead}${key(name)} = ${inline(spec)}`));
    text = text.replace(new RegExp(`^(\\s*)${esc}\\s*=\\s*\\{\\s*workspace\\s*=\\s*true\\s*(?:,\\s*(.*?))?\\s*\\}\\s*$`, "gm"), (_, lead, tail) => {
      dependencyRewrites++;
      const extra = tail ? (Bun.TOML.parse(`x = { ${tail} }`) as any).x : {};
      const merged = { ...spec, ...extra };
      const features = [...new Set([...(spec.features ?? []), ...(extra.features ?? [])])];
      if (features.length) merged.features = features;
      return `${lead}${key(name)} = ${inline(merged)}`;
    });
    if (new RegExp(`^\\[(?:[^\\]]*\\.)?(?:dependencies|dev-dependencies|build-dependencies)\\.${esc}\\]`, "m").test(text) && /workspace\s*=\s*true/.test(text))
      unhandled.push(`${file}: multi-line table for ${name}`);
  }
  if (text !== before && !dry) writeFileSync(join(root, file), text);
}
for (const m of memberSet) {
  const origin: Origin = m.startsWith("aphrody/") ? "aphrody" : m.startsWith("ay/") ? "ay" : "fw";
  if (origin !== "fw") rewriteMember(origin, m);
}

// --- patch and profile unions ---
const patch: Table = {};
for (const o of ORIGINS)
  for (const [registry, entries] of Object.entries<any>(manifests[o].patch ?? {})) {
    patch[registry] ??= {};
    for (const [name, raw] of Object.entries<any>(entries)) {
      const spec = prefixPath(o, raw);
      if (patch[registry][name] && !equal(patch[registry][name], spec)) note(`patch ${registry} ${name}: ${o} differs from the first origin, kept the first`);
      else patch[registry][name] ??= spec;
    }
  }
const profile: Table = {};
for (const o of ORIGINS)
  for (const [name, settings] of Object.entries<any>(manifests[o].profile ?? {})) {
    if (!profile[name]) profile[name] = settings;
    else {
      for (const [k, v] of Object.entries<any>(settings)) {
        // a profile name taken by an earlier origin keeps that origin settings whole (an added debug or lto key would change its binaries); only per-package overrides are added
        if (profile[name][k] === undefined && k !== "package") note(`profile ${name}.${k}: ${o} sets ${JSON.stringify(v)}, not applied (the first origin owns ${name})`);
        else if (profile[name][k] === undefined) profile[name][k] = v;
        else if (!equal(profile[name][k], v) && k !== "package") note(`profile ${name}.${k}: ${o} has ${JSON.stringify(v)}, kept ${JSON.stringify(profile[name][k])}`);
        else if (k === "package") profile[name].package = { ...v, ...profile[name].package };
      }
    }
  }

// --- the unified root manifest ---
const workspace: Table = {
  members: [...memberSet],
  exclude: [...new Set(excludes)],
  resolver: "3",
  package: manifests.fw.workspace.package,
  lints: manifests.fw.workspace.lints,
  dependencies: hoisted,
};
const header = `# One Cargo workspace for fw, aphrody and ay (tools/monorepo/unify.ts, phase 1 of docs/roadmap/2026-os-monorepo.md).
# Members keep their origin's package fields and lints (spelled out in their own manifests); dependencies shared by several members live below.
`;
const rootText =
  header +
  `[workspace]\nmembers = ${inline(workspace.members).replace(/, /g, ",\n    ").replace("[", "[\n    ").replace(/\]$/, ",\n]")}\nexclude = ${inline(workspace.exclude)}\nresolver = "3"\n\n` +
  emit(["workspace", "package"], workspace.package, false) +
  "\n" +
  emit(["workspace", "lints"], workspace.lints, false) +
  "\n[workspace.dependencies]\n" +
  Object.entries(hoisted).map(([k, v]) => `${key(k)} = ${inline(v)}`).join("\n") +
  "\n\n" +
  Object.entries(patch).map(([registry, entries]) => `[patch.${key(registry)}]\n` + Object.entries(entries as Table).map(([k, v]) => `${key(k)} = ${inline(v)}`).join("\n")).join("\n\n") +
  "\n\n" +
  Object.entries(profile).map(([name, settings]) => emit(["profile", name], settings, false)).join("\n");

// --- cargo config and toolchain ---
const cargoConfig = (o: Origin) => (existsSync(join(root, dirOf(o), ".cargo/config.toml")) ? (Bun.TOML.parse(read(`${dirOf(o)}.cargo/config.toml`)) as Table) : {});
const config = cargoConfig("fw");
const aphrody = cargoConfig("aphrody");
const ay = cargoConfig("ay");
let configText = read(".cargo/config.toml").trimEnd() + "\n";
if (aphrody.alias) for (const [k, v] of Object.entries<any>(aphrody.alias)) if (!config.alias?.[k]) configText = configText.replace(/\[alias\]\n/, `[alias]\n${key(k)} = ${inline(v)}\n`);
const zedRustflags = (aphrody["target"]?.["cfg(all())"]?.rustflags ?? []) as string[];
configText = configText.replace('rustflags = ["--cfg", "reqwest_unstable"]', `rustflags = ${inline(["--cfg", "reqwest_unstable", ...zedRustflags])}`);
for (const [section, values] of Object.entries<any>(aphrody["target"] ?? {})) {
  if (section === "cfg(all())") continue;
  const lines = Object.entries(values).map(([k, v]) => `${key(k)} = ${inline(v)}`).join("\n");
  const plain = `[target.${section}]`;
  if (configText.includes(`${plain}\n`)) configText = configText.replace(`${plain}\n`, `${plain}\n${lines}\n`);
  else configText += `\n[target.${key(section)}]\n${lines}\n`;
}
for (const section of ["env", "net", "cache"]) if (aphrody[section]) configText += `\n[${section}]\n${Object.entries(aphrody[section]).map(([k, v]) => `${key(k)} = ${inline(v)}`).join("\n")}\n`;
configText += `\n# The toolchain is pinned (rust-toolchain.toml), so the lock takes the newest release of every crate instead of the newest one a member's rust-version allows.\n[resolver]\nincompatible-rust-versions = "allow"\n`;
if (ay.env?.AY_ROOT) configText = configText.replace(/\[env\]\n/, "[env]\nAY_ROOT = { value = \"ay\", relative = true }\n");
if (ay.env?.AY_ROOT && !/\[env\]/.test(configText)) configText += `\n[env]\nAY_ROOT = { value = "ay", relative = true }\n`;

if (!dry) {
  writeFileSync(join(root, "Cargo.toml"), rootText + "\n");
  writeFileSync(join(root, ".cargo/config.toml"), configText);
  for (const f of ["aphrody/Cargo.toml", "ay/Cargo.toml", "aphrody/Cargo.lock", "ay/Cargo.lock"]) rmSync(join(root, f), { force: true });
}
// Manifest edits and bumps declared in unify.toml: [[edit]] file/from/to (regex), [upgrade] packages = [...] (cargo upgrade --incompatible).
const declared = (existsSync(overridesFile) ? Bun.TOML.parse(readFileSync(overridesFile, "utf8")) : {}) as { edit?: { file: string; from: string; to: string; why?: string }[]; upgrade?: { packages?: string[] }; pin?: { package: string; version: string; why?: string }[]; lock?: { package: string; from: string; to: string; why?: string }[] };
for (const e of declared.edit ?? []) {
  const path = join(root, e.file);
  const before = readFileSync(path, "utf8");
  const after = before.replace(new RegExp(e.from, "m"), e.to);
  note(`edit ${e.file}: ${before === after ? "NO CHANGE" : "applied"}`);
  if (!dry) writeFileSync(path, after);
}
if (!dry && declared.upgrade?.packages?.length) {
  const run = Bun.spawnSync(["cargo", "upgrade", "--incompatible", "allow", "--pinned", "allow", ...declared.upgrade.packages.flatMap((p) => ["-p", p])], { cwd: root, stdout: "pipe", stderr: "pipe" });
  note(`cargo upgrade ${declared.upgrade.packages.join(" ")}: exit ${run.exitCode}`);
}
// The lock is generated from scratch (newest release of everything), then pinned where the newest release breaks code that has not moved yet.
if (!dry) {
  const cargo = (...args: string[]) => Bun.spawnSync(["cargo", ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
  rmSync(join(root, "Cargo.lock"), { force: true });
  note(`cargo generate-lockfile: exit ${cargo("generate-lockfile").exitCode}`);
  for (const p of declared.pin ?? []) note(`pin ${p.package} to ${p.version}: exit ${cargo("update", p.package, "--precise", p.version).exitCode}`);
  let lock = readFileSync(join(root, "Cargo.lock"), "utf8");
  for (const e of declared.lock ?? []) {
    const block = new RegExp(`\\[\\[package\\]\\]\\nname = "${e.package}"\\n[\\s\\S]*?(?=\\n\\[\\[package\\]\\]|$)`);
    const found = lock.match(block)?.[0];
    note(`lock ${e.package}: ${found?.includes(e.from) ? "edited" : "NOT FOUND"}`);
    if (found) lock = lock.replace(found, found.replace(e.from, e.to));
  }
  writeFileSync(join(root, "Cargo.lock"), lock);
  note(`cargo metadata --locked: exit ${cargo("metadata", "--locked", "--format-version", "1").exitCode}`);
}
note(`members ${memberSet.size} (fw ${manifests.fw.workspace.members.length}), hoisted dependencies ${Object.keys(hoisted).length}, conflicting specs ${conflicts}`);
note(`rewrites: package fields ${packageRewrites}, inline lints ${lintRewrites}, dependency specs ${dependencyRewrites}; unhandled ${unhandled.length}`);
for (const line of [...report, ...unhandled]) console.log(line);
console.log(`${dry ? "dry run, nothing written" : "written"}: ${relative(process.cwd(), join(root, "Cargo.toml")) || "Cargo.toml"}`);
