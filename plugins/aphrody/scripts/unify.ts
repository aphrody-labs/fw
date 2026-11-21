#!/usr/bin/env bun
// Regenerates the managed part of the aphrody plugin from sources.toml: pulls every source, rewrites legacy tool calls to
// the Aphrody toolchain, dedupes by name and content hash, merges families, compacts descriptions, merges MCP and LSP
// config, pins commits in sources.lock.json and stamps the plugin version with the content hash. Idempotent.
//   bun scripts/unify.ts [--update] [--check] [--no-native] [--json]
//     --update     move every ref to its head (the cron does this); default replays the locked commits
//     --check      write nothing, exit 1 when the tree is not what the generator would produce
//     --no-native  leave the repository's own skills alone
import { cp, mkdir, rm } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { $ } from "bun";
import { FAMILIES, brokenRefs, mergeFamily, type Family } from "../../../configs/claude/scripts/merge-skill-families.ts";
import { audit, loadSkills } from "../../../configs/claude/scripts/skills.ts";
import {
  compactDescription,
  parseSkill,
  renderSkill,
  replaceDescription,
  rewriteFrontmatterPermissions,
  rewriteLoose,
  rewriteMarkdown,
  rewriteShebang,
} from "./unify-text.ts";

interface Replacement {
  from: string;
  to: string;
}
interface Source {
  id: string;
  git?: string;
  /** Directory of this repository (relative to its root) used as is, for sources without an upstream. */
  local?: string;
  optional?: boolean;
  ref?: string;
  path?: string;
  license?: string;
  skills?: string[];
  commands?: string;
  agents?: string;
  mcp?: string;
  lsp?: string;
  exclude?: string[];
  default_family?: string;
  replace?: Replacement[];
  copy?: Array<{ from: string; to: string; exclude?: string[] }>;
}
interface FamilyConfig extends Omit<Family, "children"> {
  children?: string[];
  match?: string[];
}
interface Config {
  plugin: { version: string; description_max: number };
  native: { rewrite: boolean; compact: boolean; skip: string[] };
  source: Source[];
  family: FamilyConfig[];
}
interface Lock {
  sources: Record<string, { git: string; ref: string; commit: string; license: string }>;
  managed: { skills: Record<string, string>; agents: string[]; mcp: string[]; lsp: string[] };
}

const ROOT = join(import.meta.dir, "..");
const REPO = join(ROOT, "..", "..");
const args = new Set(Bun.argv.slice(2));
const UPDATE = args.has("--update");
const CHECK = args.has("--check");
const SKIP_JUNK = /(^|\/)(\.git|\.github|node_modules|__pycache__|\.DS_Store|evals?|LICENSE(\.txt|\.md)?)$|\.pyc$/;
const TEXT = /\.(md|mdx|txt)$/;
const SCRIPT = /\.(js|mjs|cjs|ts|tsx|jsx|sh|py|ya?ml|html)$/;

const slash = (path: string): string => path.replaceAll("\\", "/");
const sha = (data: string | Uint8Array): string => new Bun.CryptoHasher("sha256").update(data).digest("hex");

async function readJson<T>(path: string, fallback: T): Promise<T> {
  const file = Bun.file(path);
  return (await file.exists()) ? ((await file.json()) as T) : fallback;
}
/** Writes only when the content changed; retries because Windows briefly locks files a running session has open. */
async function writeIfChanged(path: string, text: string): Promise<void> {
  const file = Bun.file(path);
  if ((await file.exists()) && (await file.text()) === text) return;
  for (let attempt = 0; ; attempt++) {
    try {
      await Bun.write(path, text);
      return;
    } catch (error) {
      if (attempt >= 5) throw error;
      await Bun.sleep(200 * (attempt + 1));
    }
  }
}
const writeJson = (path: string, data: unknown): Promise<void> => writeIfChanged(path, `${JSON.stringify(data, null, 2)}\n`);
async function files(dir: string, pattern = "**/*"): Promise<string[]> {
  const out: string[] = [];
  for await (const file of new Bun.Glob(pattern).scan({ cwd: dir, onlyFiles: true, dot: true })) out.push(slash(file));
  return out.sort();
}
async function treeHash(dir: string): Promise<string> {
  const hasher = new Bun.CryptoHasher("sha256");
  for (const file of await files(dir)) hasher.update(`${file}\0${sha(new Uint8Array(await Bun.file(join(dir, file)).arrayBuffer()))}\n`);
  return hasher.digest("hex");
}
async function treeBytes(dir: string): Promise<number> {
  let total = 0;
  for (const file of await files(dir)) total += Bun.file(join(dir, file)).size;
  return total;
}
const globMatch = (pattern: string, name: string): boolean => new Bun.Glob(pattern).match(name);

const config = Bun.TOML.parse(await Bun.file(join(ROOT, "sources.toml")).text()) as unknown as Config;
const lock = await readJson<Lock>(join(ROOT, "sources.lock.json"), {
  sources: {},
  managed: { skills: {}, agents: [], mcp: [], lsp: [] },
});
const cacheRoot = process.env.UNIFY_CACHE ?? join(process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"), "aphrody-unify");
const stage = join(cacheRoot, "stage");
const log: string[] = [];
const stats = { rewrites: 0, compacted: 0, collisions: [] as string[], duplicates: [] as string[], unmerged: [] as string[] };

async function resolve(source: Source): Promise<string | undefined> {
  if (source.local !== undefined) {
    const dir = join(REPO, source.local, source.path ?? "");
    // An optional local source is absorbed once it is committed, so the cron (which only sees the repository) agrees with this checkout.
    if (source.optional && (await $`git -C ${REPO} ls-files -- ${source.local}`.text()).trim() === "") return undefined;
    if ((await files(dir).catch(() => [] as string[])).length === 0) {
      if (source.optional) return undefined;
      throw new Error(`local source ${source.id}: ${dir} is missing`);
    }
    lock.sources[source.id] = { git: `local:${source.local}`, ref: "", commit: (await treeHash(dir)).slice(0, 12), license: source.license ?? "" };
    return dir;
  }
  if (!source.git || !source.ref) throw new Error(`source ${source.id}: git and ref are required`);
  const dir = join(cacheRoot, "git", source.git.replace("/", "__"));
  if (!(await Bun.file(join(dir, ".git", "HEAD")).exists())) {
    await mkdir(dir, { recursive: true });
    await $`git init -q ${dir}`.quiet();
    await $`git -C ${dir} remote add origin https://github.com/${source.git}.git`.quiet();
  }
  const locked = lock.sources[source.id]?.commit;
  const target = !UPDATE && locked ? locked : source.ref;
  await $`git -C ${dir} fetch -q --depth 1 origin ${target}`.quiet();
  await $`git -C ${dir} checkout -q --force FETCH_HEAD`.quiet();
  const commit = (await $`git -C ${dir} rev-parse HEAD`.text()).trim();
  lock.sources[source.id] = { git: source.git, ref: source.ref, commit, license: source.license ?? "" };
  return join(dir, source.path ?? "");
}

function applyReplacements(text: string, replacements: Replacement[] = []): string {
  return replacements.reduce((acc, { from, to }) => acc.replaceAll(from, to), text);
}

/** Copies a skill directory into the stage, normalising SKILL.md and rewriting legacy tools in text and shebangs. */
async function stageSkill(from: string, to: string, name: string, source: Source): Promise<void> {
  await rm(to, { recursive: true, force: true });
  for (const file of await files(from)) {
    if (SKIP_JUNK.test(file) || file.split("/").some((part) => SKIP_JUNK.test(part))) continue;
    const target = join(to, file);
    const isSkillFile = file === "SKILL.md";
    if (isSkillFile || TEXT.test(file)) {
      let text = applyReplacements(await Bun.file(join(from, file)).text(), source.replace);
      if (isSkillFile) {
        const skill = parseSkill(text);
        const { line, rest } = compactDescription(skill.frontmatter.description ?? "", config.plugin.description_max);
        if (rest) stats.compacted++;
        skill.frontmatter.name = name;
        skill.frontmatter.description = line;
        if (skill.frontmatter["allowed-tools"]) skill.frontmatter["allowed-tools"] = rewriteFrontmatterPermissions(skill.frontmatter["allowed-tools"]);
        if (rest) skill.body = `> ${rest}\n\n${skill.body}`;
        text = renderSkill(skill);
      }
      const markdown = rewriteMarkdown(text);
      const rewritten = rewriteLoose(markdown.text);
      stats.rewrites += markdown.changes + rewritten.changes;
      await Bun.write(target, rewritten.text);
    } else if (SCRIPT.test(file) || (!/\.[^/]+$/.test(file) && Bun.file(join(from, file)).size < 262144 && (await Bun.file(join(from, file)).slice(0, 2).text()) === "#!")) {
      const rewritten = rewriteLoose(rewriteShebang(await Bun.file(join(from, file)).text()));
      stats.rewrites += rewritten.changes;
      await Bun.write(target, rewritten.text);
    } else await Bun.write(target, Bun.file(join(from, file)));
  }
}

async function collect(source: Source, root: string, taken: Map<string, string>): Promise<Set<string>> {
  const names = new Set<string>();
  const offer = async (name: string, dir: string) => {
    if (source.exclude?.includes(name)) return;
    const owner = taken.get(name);
    if (owner) {
      const probe = join(stage, "probe", name);
      await stageSkill(dir, probe, name, source);
      const same = (await treeHash(probe)) === (await treeHash(join(stage, "skills", name)));
      (same ? stats.duplicates : stats.collisions).push(`${name}: ${source.id} vs ${owner}`);
      await rm(probe, { recursive: true, force: true });
      return;
    }
    await stageSkill(dir, join(stage, "skills", name), name, source);
    taken.set(name, source.id);
    names.add(name);
  };
  for (const pattern of source.skills ?? []) {
    for (const file of await files(root, pattern)) {
      if (file.split("/").some((part) => [".agents", ".github", ".claude-plugin", "node_modules"].includes(part))) continue;
      const dir = join(root, dirname(file));
      const skill = parseSkill(await Bun.file(join(root, file)).text());
      const name = (skill.frontmatter.name ?? "").replace(/^.*::/, "") || dirname(file).split("/").at(-1) || "";
      await offer(name, dir);
    }
  }
  if (source.commands) {
    for (const file of await files(join(root, source.commands), "*.md")) {
      const text = await Bun.file(join(root, source.commands, file)).text();
      const name = file.replace(/\.md$/, "");
      const skill = parseSkill(text);
      skill.frontmatter.name = name;
      const holder = join(cacheRoot, "commands", source.id, name);
      await rm(holder, { recursive: true, force: true });
      await Bun.write(join(holder, "SKILL.md"), renderSkill(skill));
      await offer(name, holder);
    }
  }
  return names;
}

async function mergeJsonMap(path: string, key: string, previous: string[], incoming: Record<string, unknown>): Promise<string[]> {
  const json = await readJson<Record<string, Record<string, unknown>>>(path, {});
  const map = json[key] ?? {};
  for (const stale of previous) delete map[stale];
  Object.assign(map, incoming);
  json[key] = map;
  if (!CHECK) await writeJson(path, json);
  return Object.keys(incoming).sort();
}

// ---- main ------------------------------------------------------------------------------------------------------------
await rm(stage, { recursive: true, force: true });
await mkdir(join(stage, "skills"), { recursive: true });
const taken = new Map<string, string>();
const bySource = new Map<string, Set<string>>();
const agents = new Map<string, string>();
const mcpIncoming: Record<string, unknown> = {};
const lspIncoming: Record<string, unknown> = {};
const roots = new Map<string, string>();

for (const source of config.source) {
  const root = await resolve(source);
  if (root === undefined) continue;
  roots.set(source.id, root);
  bySource.set(source.id, await collect(source, root, taken));
  if (source.agents) {
    for (const file of await files(join(root, source.agents), "*.md")) {
      agents.set(file, applyReplacements(await Bun.file(join(root, source.agents, file)).text(), source.replace));
    }
  }
  if (source.mcp) {
    const mcp = await readJson<{ mcpServers?: Record<string, unknown> }>(join(root, source.mcp), {});
    Object.assign(mcpIncoming, mcp.mcpServers ?? {});
  }
  if (source.lsp) {
    const lsp = await readJson<{ lspServers?: Record<string, unknown> } & Record<string, unknown>>(join(root, source.lsp), {});
    Object.assign(lspIncoming, lsp.lspServers ?? {});
  }
}

const stagedNames = (): string[] => [...taken.keys()];
const families: Family[] = config.family.map(({ match, children, ...rest }) => {
  const matched = (match ?? []).flatMap((pattern) => stagedNames().filter((name) => globMatch(pattern, name)));
  return { ...rest, children: [...new Set([...(children ?? []), ...matched])] };
});
const claimed = new Set(families.flatMap((family) => family.children));
for (const source of config.source) {
  const target = families.find((family) => family.parent === source.default_family);
  if (!target) continue;
  for (const name of bySource.get(source.id) ?? []) {
    if (!claimed.has(name) && name !== target.parent) {
      target.children.push(name);
      claimed.add(name);
      stats.unmerged.push(`${name} -> ${target.parent}`);
    }
  }
}
for (const family of families) {
  const result = mergeFamily(join(stage, "skills"), family);
  const broken = brokenRefs(join(stage, "skills"), family.parent);
  if (broken.length) log.push(`broken refs in ${family.parent}: ${broken.join(", ")}`);
  if (result.merged.length === 0 && result.missing.length === family.children.length) log.push(`family ${family.parent}: no children`);
}
for (const source of config.source) {
  for (const { from, to, exclude = [] } of source.copy ?? []) {
    const root = roots.get(source.id) ?? "";
    const entries = await files(join(root, from)).catch(() => [] as string[]);
    for (const file of entries) {
      if (exclude.some((pattern) => globMatch(pattern, file))) continue;
      const target = join(stage, to, file);
      if (TEXT.test(file) || SCRIPT.test(file) || /\.(html|json)$/.test(file)) {
        const text = applyReplacements(await Bun.file(join(root, from, file)).text(), source.replace);
        await Bun.write(target, rewriteLoose(TEXT.test(file) ? rewriteMarkdown(text).text : text).text);
      } else await Bun.write(target, Bun.file(join(root, from, file)));
    }
  }
}

// Compaction of router descriptions (merged parents) and a final audit of the staged tree.
const stagedParents = await files(join(stage, "skills"), "*/SKILL.md");
for (const file of stagedParents) {
  const path = join(stage, "skills", file);
  const text = await Bun.file(path).text();
  const { frontmatter } = parseSkill(text);
  const { line } = compactDescription(frontmatter.description ?? "", config.plugin.description_max);
  if (line !== (frontmatter.description ?? "")) await Bun.write(path, replaceDescription(text, line));
}

// ---- write the managed files into the plugin -----------------------------------------------------------------------
const outSkills = join(ROOT, "skills");
const managedNow: Record<string, string> = {};
const changed: string[] = [];
const nativeWins: string[] = [];
for (const name of (await Array.fromAsync(new Bun.Glob("*/SKILL.md").scan({ cwd: join(stage, "skills") }))).map((file) => slash(file).split("/")[0] ?? "")) {
  const hash = await treeHash(join(stage, "skills", name));
  const exists = await Bun.file(join(outSkills, name, "SKILL.md")).exists();
  if (exists && lock.managed.skills[name] === undefined) {
    nativeWins.push(name);
    continue;
  }
  managedNow[name] = hash;
  if (lock.managed.skills[name] === hash && exists) continue;
  changed.push(name);
  if (CHECK) continue;
  await rm(join(outSkills, name), { recursive: true, force: true });
  await cp(join(stage, "skills", name), join(outSkills, name), { recursive: true });
}
const pruned = Object.keys(lock.managed.skills).filter((name) => managedNow[name] === undefined);
if (!CHECK) for (const name of pruned) await rm(join(outSkills, name), { recursive: true, force: true });

const agentNames = [...agents.keys()].sort();
for (const stale of lock.managed.agents.filter((name) => !agents.has(name))) if (!CHECK) await rm(join(ROOT, "agents", stale), { force: true });
if (!CHECK) for (const [file, text] of agents) await Bun.write(join(ROOT, "agents", file), text);

const manifestMcp = await mergeJsonMap(join(ROOT, ".claude-plugin", "mcp.json"), "mcpServers", lock.managed.mcp, mcpIncoming);
const lspPath = join(ROOT, ".lsp.json");
const lspJson = await readJson<Record<string, unknown>>(lspPath, {});
for (const stale of lock.managed.lsp) delete lspJson[stale];
Object.assign(lspJson, lspIncoming);
if (!CHECK) await writeJson(lspPath, lspJson);

// Licenses of every absorbed source, copied once instead of once per skill.
const notices = ["# Third-party sources", "", "Generated by `scripts/unify.ts`; each source keeps its upstream license under `licenses/`.", "", "| Source | Repository | Commit | License |", "| --- | --- | --- | --- |"];
for (const source of config.source) {
  const entry = lock.sources[source.id];
  if (!entry || !source.git) continue;
  notices.push(`| ${source.id} | https://github.com/${entry.git} | ${entry.commit.slice(0, 12)} | ${entry.license} |`);
  const root = join(cacheRoot, "git", source.git.replace("/", "__"));
  for (const name of ["LICENSE", "LICENSE.txt", "LICENSE.md"]) {
    const file = Bun.file(join(root, name));
    if (await file.exists()) {
      if (!CHECK) await Bun.write(join(ROOT, "licenses", `${source.id}.txt`), file);
      break;
    }
  }
}
if (!CHECK) await Bun.write(join(ROOT, "THIRD_PARTY.md"), `${notices.join("\n")}\n`);

// ---- native skills: same rewrite and compaction, never merged away -------------------------------------------------
const nativeReport = { rewritten: 0, compacted: 0, skippedDirty: 0 };
if (config.native.rewrite && !args.has("--no-native")) {
  const dirty = (await $`git -C ${REPO} status --porcelain`.text())
    .split("\n")
    .filter(Boolean)
    .map((line) => slash(line.slice(3)).split(" -> ").at(-1)?.replace(/^"|"$/g, "") ?? "");
  const prefix = `${slash(ROOT).replace(slash(REPO), "").replace(/^\//, "")}/skills/`;
  const isDirty = (rel: string): boolean => dirty.some((entry) => entry === prefix + rel || (entry.endsWith("/") && (prefix + rel).startsWith(entry)));
  for (const file of await files(outSkills, "**/*.md")) {
    const top = file.split("/")[0] ?? "";
    if (managedNow[top] !== undefined || config.native.skip.includes(top)) continue;
    if (isDirty(file)) {
      nativeReport.skippedDirty++;
      continue;
    }
    const path = join(outSkills, file);
    let text = await Bun.file(path).text();
    const original = text;
    if (config.native.compact && /^[^/]+\/SKILL\.md$/.test(file)) {
      const { frontmatter } = parseSkill(text);
      const { line } = compactDescription(frontmatter.description ?? "", config.plugin.description_max);
      const block = /^---\r?\n([\s\S]*?)\r?\n---/.exec(text)?.[1] ?? "";
      const strict = (() => {
        try {
          return typeof (Bun.YAML.parse(block) as { description?: unknown } | null)?.description === "string";
        } catch {
          return false;
        }
      })();
      if (frontmatter.description !== undefined && (line !== frontmatter.description || !strict)) {
        const { rest } = compactDescription(frontmatter.description, config.plugin.description_max);
        text = replaceDescription(text, line);
        if (rest) text = text.replace(/^(---\r?\n[\s\S]*?\r?\n---\r?\n)/, `$1> ${rest}\n\n`);
        nativeReport.compacted++;
      }
    }
    const rewritten = rewriteMarkdown(text);
    text = rewritten.text;
    nativeReport.rewritten += rewritten.changes;
    if (text !== original && !CHECK) await Bun.write(path, text);
  }
}

// ---- version stamp, lock, report -----------------------------------------------------------------------------------
const hashed = ["skills", "agents", "hooks", "mcp", "lib", "output-styles", "licenses"];
const contentHasher = new Bun.CryptoHasher("sha256");
for (const dir of hashed) contentHasher.update(await treeHash(join(ROOT, dir)).catch(() => ""));
for (const file of [".lsp.json", ".claude-plugin/mcp.json", ".claude-plugin/hooks.json"]) {
  contentHasher.update(await Bun.file(join(ROOT, file)).text().catch(() => ""));
}
const version = `${config.plugin.version}-${contentHasher.digest("hex").slice(0, 8)}`;
if (!CHECK) {
  const manifestPath = join(ROOT, ".claude-plugin", "plugin.json");
  const manifest = await readJson<Record<string, unknown>>(manifestPath, {});
  manifest.version = version;
  manifest.agents = (await files(join(ROOT, "agents"), "*.md")).map((file) => `./agents/${file}`);
  await writeJson(manifestPath, manifest);
  const marketplacePath = join(REPO, ".claude-plugin", "marketplace.json");
  const marketplace = await readJson<{ plugins: Array<{ name: string; version?: string }> }>(marketplacePath, { plugins: [] });
  for (const entry of marketplace.plugins) if (entry.name === "aphrody") entry.version = version;
  await writeJson(marketplacePath, marketplace);
}
lock.managed = { skills: Object.fromEntries(Object.entries(managedNow).sort(([a], [b]) => a.localeCompare(b))), agents: agentNames, mcp: manifestMcp, lsp: Object.keys(lspIncoming).sort() };
lock.sources = Object.fromEntries(Object.entries(lock.sources).sort(([a], [b]) => a.localeCompare(b)));
if (!CHECK) await writeJson(join(ROOT, "sources.lock.json"), lock);

const skills = loadSkills(outSkills, "aphrody");
const listing = skills.filter((skill) => !skill.hidden).map((skill) => `${skill.name}\n${skill.description.slice(0, 200)}`);
const listingBytes = new TextEncoder().encode(listing.join("\n")).length;
const issues = audit(skills).filter((issue) => issue.kind !== "near-duplicate");
const summary = {
  version,
  skills: skills.length,
  listingBytes,
  listingTokensEstimate: Math.round(listingBytes / 4),
  pluginBytes: await treeBytes(ROOT),
  changed: changed.length,
  pruned: pruned.length,
  nativeWins,
  legacyRewrites: stats.rewrites,
  descriptionsCompacted: stats.compacted,
  native: nativeReport,
  duplicatesDropped: stats.duplicates,
  nameCollisions: stats.collisions,
  autoMergedNew: stats.unmerged,
  auditIssues: issues.map((issue) => `${issue.kind}: ${issue.skill} ${issue.detail}`),
  log,
};
console.log(JSON.stringify(summary, null, 2));
process.exit(CHECK && (changed.length > 0 || pruned.length > 0) ? 1 : issues.length > 0 ? 1 : 0);
