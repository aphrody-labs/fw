#!/usr/bin/env bun
// Recovery inventory: everything the legacy monorepo cleanup may have lost, from every place it can survive.
// Sources: crates.io (crates owned by the owner account of ay.toml), npm (the registry org scope and the packages of the
// logged-in npm account), GitHub (archived repositories and non-fork originals of the orgs of ay.toml), the legacy
// inventory (.private/legacy-inventory.toml) and the trees under /opt and /srv of the hosts of ay.toml [hosts].
// Each unit is classified against the manifests of the current repositories (ay.toml [repos] checkouts):
//   live  the unit exists in a current repository (`where`), or is an upstream fork that is tracked as such
//   lost  our own code that survives only in a registry, an archive, the legacy repository or a host tree (`home` says
//         where it belongs in the current architecture, from docs/research/recovery-rules.toml)
//   dead  no value: regenerable build output, toolchains, duplicates, renamed upstream copies
//   bun tools/recover.ts [--cache <dir>] [--offline] [--out <file>] [--md <file>] (defaults in .private/)
//   --cache keeps the raw registry, GitHub and host snapshots (default: <tmp>/ay-recover); --offline reuses them.
import { privatePath } from "./local.ts";
import { readAy } from "./local.ts";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");

export type State = "live" | "lost" | "dead";
export type Origin = "crate" | "npm" | "repo" | "legacy" | "vps";
export type Unit = {
  origin: Origin;
  name: string;
  kind?: string;
  version?: string;
  repository?: string;
  description?: string;
  state: State;
  where?: string;
  archive?: string;
  home?: string;
  note?: string;
};
export type Manifest = { repo: string; ecosystem: "cargo" | "npm"; name: string; path: string };
export type Rule = {
  origin?: Origin;
  kind?: string;
  name?: string;
  repository?: string;
  description?: string;
  state?: State;
  home?: string;
  where?: string;
  note?: string;
};
export type Rules = { rule: Rule[] };

type Toml = Record<string, any>;
const readToml = (path: string): Toml => Bun.TOML.parse(readFileSync(path, "utf8")) as Toml;

/** Identity used to match a published name with a manifest of a current repository. */
export function norm(name: string): string {
  return name.toLowerCase().replace(/^@[^/]+\//, "").replace(/^(ay|aphrody|bunos?)[_-]/, "").replace(/[_-]/g, "");
}

/** Package names of Cargo.toml and package.json texts: `[package] name` and `"name"`. */
export function manifestName(path: string, text: string): string | undefined {
  try {
    if (path.endsWith("Cargo.toml")) return (Bun.TOML.parse(text) as Toml).package?.name;
    if (path.endsWith("package.json")) return JSON.parse(text).name;
  } catch {}
  return undefined;
}

export class LiveIndex {
  private exact = new Map<string, Manifest>();
  private loose = new Map<string, Manifest>();
  constructor(manifests: Manifest[]) {
    for (const m of manifests) {
      const key = `${m.ecosystem}:${m.name}`;
      if (!this.exact.has(key)) this.exact.set(key, m);
      const k = `${m.ecosystem}:${norm(m.name)}`;
      if (!this.loose.has(k)) this.loose.set(k, m);
    }
  }
  get size() {
    return this.exact.size;
  }
  find(ecosystem: Manifest["ecosystem"], name: string): { m: Manifest; exact: boolean } | undefined {
    const e = this.exact.get(`${ecosystem}:${name}`);
    if (e) return { m: e, exact: true };
    const l = this.loose.get(`${ecosystem}:${norm(name)}`);
    return l && norm(name).length > 3 ? { m: l, exact: false } : undefined;
  }
}

/** Paths that hold upstream copies, fixtures or build output: their manifests are not units of the repository. */
export const foreign = (path: string) => /(^|\/)(node_modules|vendor|third_party|test|tests|fixtures|testdata|examples|target)\//.test(path);

/** Manifests tracked by a checkout, one `git ls-files` pass. */
export async function scanCheckout(repo: string, dir: string): Promise<Manifest[]> {
  const p = Bun.spawn(["git", "-C", dir, "ls-files", "--", "*Cargo.toml", "*package.json"], { stdout: "pipe", stderr: "ignore" });
  const files = (await new Response(p.stdout).text()).split("\n").filter(f => f && !foreign(f));
  const out: Manifest[] = [];
  for (const f of files) {
    const name = manifestName(f, await Bun.file(join(dir, f)).text().catch(() => ""));
    if (name) out.push({ repo, ecosystem: f.endsWith("Cargo.toml") ? "cargo" : "npm", name, path: f });
  }
  return out;
}

/** Manifests of a git revision (a repository we cannot or should not check out), through one `git cat-file --batch`. */
export async function scanRevision(repo: string, dir: string, rev: string): Promise<Manifest[]> {
  const run = async (args: string[], stdin?: string) => {
    const p = Bun.spawn(["git", "-C", dir, ...args], { stdin: stdin === undefined ? "ignore" : new TextEncoder().encode(stdin), stdout: "pipe", stderr: "ignore" });
    return Buffer.from(await new Response(p.stdout).arrayBuffer());
  };
  const files = (await run(["ls-tree", "-r", "--name-only", rev])).toString().split("\n").filter(f => /(^|\/)(Cargo\.toml|package\.json)$/.test(f));
  const buf = await run(["cat-file", "--batch"], files.map(f => `${rev}:${f}`).join("\n") + "\n");
  const out: Manifest[] = [];
  let pos = 0;
  for (const f of files) {
    const nl = buf.indexOf(10, pos);
    const [, type, size] = buf.subarray(pos, nl).toString().split(" ");
    if (type !== "blob") { pos = nl + 1; continue; }
    const name = manifestName(f, buf.subarray(nl + 1, nl + 1 + Number(size)).toString());
    pos = nl + 1 + Number(size) + 1;
    if (name) out.push({ repo, ecosystem: f.endsWith("Cargo.toml") ? "cargo" : "npm", name, path: f });
  }
  return out;
}

function matches(rule: Rule, u: Unit): boolean {
  for (const k of ["origin", "kind"] as const) if (rule[k] !== undefined && rule[k] !== u[k]) return false;
  for (const k of ["name", "repository", "description"] as const) if (rule[k] !== undefined && !new RegExp(rule[k]!, "i").test(u[k] ?? "")) return false;
  return true;
}

/** First rule that matches the unit and says something (state, home or note). */
export function ruleFor(u: Unit, rules: Rule[]): Rule | undefined {
  return rules.find(r => matches(r, u));
}

export type Context = {
  live: LiveIndex;
  /** Fork repositories of the orgs (upstream copies that are tracked as forks, not as our code). */
  forks: Set<string>;
  /** Our own sources that survive in an archive (repository name → manifests), by ecosystem and name. */
  archives: LiveIndex;
  rules: Rule[];
  /** Directories of the current repositories (up to three levels), by identity: where a unit without manifest lives. */
  dirs?: Map<string, string>;
};

/** Directory names that carry no identity: they exist in every repository. */
const GENERIC = new Set(["scripts", "assets", "tests", "docs", "build", "tools", "examples", "common", "shared", "utils", "cli", "core", "lib", "libs", "config", "main"]);

/** Directory index of a repository: every tracked directory up to three levels deep, keyed by the identity of its name. */
export function dirIndex(repo: string, files: string[], into = new Map<string, string>()): Map<string, string> {
  for (const f of files) {
    if (foreign(f)) continue;
    const parts = f.split("/").slice(0, -1);
    if (parts[0]?.startsWith(".") || parts.includes("docs")) continue;
    for (let i = 1; i <= Math.min(parts.length, 3); i++) {
      const path = parts.slice(0, i).join("/");
      if (!into.has(`path:${path}`)) into.set(`path:${path}`, `${repo}:${path}`);
      const k = norm(parts[i - 1]!);
      if (i <= 2 && k.length > 3 && !GENERIC.has(k) && !into.has(k)) into.set(k, `${repo}:${path}`);
    }
  }
  return into;
}

/** Classify one unit against the current repositories, the archives and the rules. */
export function classify(u: Unit, ctx: Context): Unit {
  const out = { ...u };
  const eco = u.origin === "npm" ? "npm" : "cargo";
  if (u.origin === "crate" || u.origin === "npm") {
    const f = ctx.live.find(eco, u.name);
    if (f) {
      return { ...out, state: "live", where: `${f.m.repo}:${f.m.path.replace(/\/?(Cargo|package)\.(toml|json)$/, "") || "."}`, note: f.exact ? undefined : `matched by identity ${norm(u.name)}` };
    }
    const fork = [...ctx.forks].find(r => r.length > 4 && (u.name === r || u.name.startsWith(r + "-") || u.name.startsWith(r + "_")));
    if (fork) return { ...out, state: "live", where: `fork:${fork}`, note: "upstream fork, not our code" };
    const a = ctx.archives.find(eco, u.name);
    if (a) out.archive = `${a.m.repo}:${a.m.path.replace(/\/?(Cargo|package)\.(toml|json)$/, "")}`;
  }
  const r = ruleFor(out, ctx.rules);
  out.state = r?.state ?? "lost";
  if (r?.home) out.home = r.home;
  if (r?.where) out.where = r.where;
  if (r?.note) out.note = r.note;
  if (out.state === "lost" && !out.home) out.home = "review";
  return out;
}

const q = (s: string) => JSON.stringify(s);

export function toToml(units: Unit[], meta: Record<string, string>): string {
  const lines = ["# Generated by `bun tools/recover.ts`; rerun it instead of editing. Rules: docs/research/recovery-rules.toml.", ""];
  lines.push("[meta]", ...Object.entries(meta).map(([k, v]) => `${k} = ${q(v)}`), "");
  for (const u of units) {
    lines.push("[[unit]]");
    for (const [k, v] of Object.entries(u)) if (v !== undefined && v !== "") lines.push(`${k} = ${q(String(v))}`);
    lines.push("");
  }
  return lines.join("\n");
}

export function counts(units: Unit[]): Map<string, Record<State, number>> {
  const m = new Map<string, Record<State, number>>();
  for (const u of units) {
    const r = m.get(u.origin) ?? { live: 0, lost: 0, dead: 0 };
    r[u.state]++;
    m.set(u.origin, r);
  }
  return m;
}

export function toMarkdown(units: Unit[], meta: Record<string, string>): string {
  const out = ["# Recovery inventory", "", `Generated by \`bun tools/recover.ts\` on ${meta.scanned}; data in [recovery.toml](recovery.toml), rules in [recovery-rules.toml](recovery-rules.toml).`, ""];
  out.push("| origin | live | lost | dead | total |", "| :-- | --: | --: | --: | --: |");
  const tot = { live: 0, lost: 0, dead: 0 };
  for (const [o, r] of [...counts(units)].sort()) {
    out.push(`| ${o} | ${r.live} | ${r.lost} | ${r.dead} | ${r.live + r.lost + r.dead} |`);
    for (const s of ["live", "lost", "dead"] as const) tot[s] += r[s];
  }
  out.push(`| all | ${tot.live} | ${tot.lost} | ${tot.dead} | ${tot.live + tot.lost + tot.dead} |`, "");
  const lost = units.filter(u => u.state === "lost");
  const homes = new Map<string, Unit[]>();
  for (const u of lost) homes.set(u.home ?? "review", [...(homes.get(u.home ?? "review") ?? []), u]);
  out.push("## Lost units by home", "", "| home | lost | crates.io | npm | repo | legacy | vps |", "| :-- | --: | --: | --: | --: | --: | --: |");
  for (const [h, us] of [...homes].sort((a, b) => b[1].length - a[1].length)) {
    const c = (o: Origin) => us.filter(u => u.origin === o).length;
    out.push(`| ${h} | ${us.length} | ${c("crate")} | ${c("npm")} | ${c("repo")} | ${c("legacy")} | ${c("vps")} |`);
  }
  out.push("");
  for (const [h, us] of [...homes].sort()) {
    out.push(`### ${h}`, "");
    for (const u of us.slice(0, 400)) out.push(`- ${u.origin} \`${u.name}\`${u.version ? ` ${u.version}` : ""}${u.archive ? ` (archive ${u.archive})` : ""}${u.note ? `: ${u.note}` : ""}`);
    if (us.length > 400) out.push(`- ... ${us.length - 400} more in recovery.toml`);
    out.push("");
  }
  return out.join("\n");
}

// ---------------------------------------------------------------- collectors

export type Fetcher = (url: string) => Promise<any>;
const UA = "ay-recover (https://github.com/aphrody-dev/ay)";
export const fetchJson: Fetcher = async url => {
  const r = await fetch(url, { headers: { "user-agent": UA, accept: "application/json" } });
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return r.json();
};

type Crate = { name: string; version: string; repository?: string; description?: string; yanked: boolean };
/** Every crate owned by a crates.io user (the API allows one request per second). */
export async function collectCrates(login: string, get: Fetcher = fetchJson, pause = 1100): Promise<Crate[]> {
  const id = (await get(`https://crates.io/api/v1/users/${login}`)).user.id;
  const out: Crate[] = [];
  for (let page = 1; ; page++) {
    if (page > 1) await Bun.sleep(pause);
    const j = await get(`https://crates.io/api/v1/crates?user_id=${id}&per_page=100&page=${page}&sort=alpha`);
    for (const c of j.crates) out.push({ name: c.name, version: c.max_version ?? c.newest_version, repository: c.repository ?? undefined, description: c.description ?? undefined, yanked: !!c.yanked });
    if (j.crates.length < 100) return out;
  }
}

type NpmPkg = { name: string; version?: string; repository?: string; description?: string; deprecated?: boolean };
export function npmRepository(p: any): string | undefined {
  const r = typeof p?.repository === "string" ? p.repository : p?.repository?.url;
  return r?.replace(/^git\+/, "").replace(/\.git$/, "");
}
/** Packages of an org scope (`npm access list packages`) and of the logged-in account, with their latest manifest. */
export async function collectNpm(scope: string, get: Fetcher = fetchJson, sh = (cmd: string[]) => Bun.$`${cmd}`.quiet().text()): Promise<NpmPkg[]> {
  const names = new Set<string>();
  try {
    for (const n of Object.keys(JSON.parse(await sh(["npm", "access", "list", "packages", scope.replace(/^@/, ""), "--json"])))) names.add(n);
  } catch {}
  const user = (await sh(["npm", "whoami"]).catch(() => "")).trim();
  for (let from = 0; user; from += 250) {
    const j = await get(`https://registry.npmjs.org/-/v1/search?text=maintainer:${user}&size=250&from=${from}`);
    for (const o of j.objects) names.add(o.package.name);
    if (j.objects.length < 250) break;
  }
  const list = [...names].sort();
  const out: NpmPkg[] = [];
  let next = 0;
  await Promise.all(
    Array.from({ length: 8 }, async () => {
      while (next < list.length) {
        const name = list[next++]!;
        try {
          const p = await get(`https://registry.npmjs.org/${name.replace("/", "%2f")}`);
          const v = p.versions?.[p["dist-tags"]?.latest];
          out.push({ name, version: p["dist-tags"]?.latest, repository: npmRepository(v ?? p), description: v?.description ?? p.description, deprecated: !!v?.deprecated });
        } catch {
          out.push({ name });
        }
      }
    }),
  );
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

export type Repo = { name: string; org: string; archived: boolean; fork: boolean; description?: string; pushedAt?: string; sizeKb?: number };
export async function collectRepos(orgs: string[], gh = (org: string) => Bun.$`gh repo list ${org} --limit 1000 --json name,isArchived,isFork,description,pushedAt,diskUsage`.quiet().text()): Promise<Repo[]> {
  const out: Repo[] = [];
  for (const org of orgs) {
    for (const r of JSON.parse(await gh(org))) out.push({ name: r.name, org, archived: r.isArchived, fork: r.isFork, description: r.description || undefined, pushedAt: r.pushedAt, sizeKb: r.diskUsage });
  }
  return out;
}

export type Tree = { host: string; path: string; mb: number; link?: string; remote?: string; head?: string; dirty: number; ahead: number; mtime: string };
/** The shell run on a host: tools/recover-host.sh, one TSV line per top-level tree of /opt, /srv and the work volume. */
export const hostScript = () => readFileSync(join(import.meta.dir, "recover-host.sh"), "utf8");

export function parseTrees(host: string, tsv: string): Tree[] {
  return tsv
    .split("\n")
    .filter(Boolean)
    .map(l => l.split("\t"))
    .filter(f => f.length >= 8)
    .map(([path, mb, link, remote, head, dirty, ahead, mtime]) => ({ host, path: path!, mb: Number(mb), link: link || undefined, remote: remote || undefined, head: head || undefined, dirty: Number(dirty), ahead: Number(ahead), mtime: mtime! }));
}

export async function collectTrees(hosts: Record<string, string>, work: Record<string, string>, roots: string[], ssh = async (host: string, script: string, work: string) => {
  const p = Bun.spawn(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15", host, `bash -s -- ${work} ${roots.join(" ")}`], { stdin: new TextEncoder().encode(script), stdout: "pipe", stderr: "ignore" });
  return new Response(p.stdout).text();
}): Promise<Tree[]> {
  const out: Tree[] = [];
  const seen = new Set<string>();
  for (const [role, host] of Object.entries(hosts)) {
    if (seen.has(host)) continue;
    seen.add(host);
    out.push(...parseTrees(host, await ssh(host, hostScript(), work[role] ?? "/nonexistent")));
  }
  return out;
}

// ---------------------------------------------------------------- assembly

export function loadRules(path = join(root, "docs/research/recovery-rules.toml")): Rule[] {
  return existsSync(path) ? ((readToml(path) as Partial<Rules>).rule ?? []) : [];
}

/** Units of the recovery inventory from the collected snapshots (pure: no network, no disk). */
export function assemble(input: {
  crates: Crate[];
  npm: NpmPkg[];
  repos: Repo[];
  legacy: { kind: string; name: string; path: string; description?: string; product: string; state: string; url?: string; fork?: string }[];
  trees: Tree[];
}, ctx: Context): Unit[] {
  const units: Unit[] = [];
  for (const c of input.crates) units.push(classify({ origin: "crate", name: c.name, kind: "crate", version: c.version, repository: c.repository, description: c.description, state: "lost", note: c.yanked ? "yanked" : undefined }, ctx));
  for (const p of input.npm) units.push(classify({ origin: "npm", name: p.name, kind: "package", version: p.version, repository: p.repository, description: p.description, state: "lost", note: p.deprecated ? "deprecated" : undefined }, ctx));
  for (const r of input.repos) {
    if (!r.archived) continue;
    units.push(classify({ origin: "repo", name: `${r.org}/${r.name}`, kind: "archived", description: r.description, repository: `https://github.com/${r.org}/${r.name}`, state: "lost" }, ctx));
  }
  for (const l of input.legacy) {
    const eco = l.kind === "crate" ? "cargo" : l.kind === "package" ? "npm" : undefined;
    const f = eco ? ctx.live.find(eco, l.name) : undefined;
    const u: Unit = { origin: "legacy", name: l.name, kind: l.kind, description: l.description, repository: l.path, state: "lost" };
    if (f) units.push({ ...u, state: "live", where: `${f.m.repo}:${f.m.path.replace(/\/?(Cargo|package)\.(toml|json)$/, "")}` });
    else if (!eco && l.kind !== "third_party" && (ctx.dirs?.get(`path:${l.path}`) ?? ctx.dirs?.get(norm(l.path.split("/").pop()!)))) units.push({ ...u, state: "live", where: ctx.dirs.get(`path:${l.path}`) ?? ctx.dirs.get(norm(l.path.split("/").pop()!)), note: "directory of the same path or name" });
    else if (l.kind === "third_party") units.push({ ...u, state: "live", where: `fork:${l.fork ?? l.url ?? ""}`, note: "third-party pin, tracked as a fork" });
    else units.push(classify(u, ctx));
  }
  for (const t of input.trees) {
    const u: Unit = { origin: "vps", name: `${t.host}:${t.path}`, kind: t.link ? "link" : t.remote ? "git" : "tree", version: t.head, repository: t.remote, description: `${t.mb} MB, modified ${t.mtime}${t.dirty ? `, ${t.dirty} uncommitted files` : ""}${t.ahead ? `, ${t.ahead} unpushed commits` : ""}`, state: "lost" };
    units.push(classify(u, ctx));
  }
  return units;
}

if (import.meta.main) {
  const arg = (k: string) => {
    const i = process.argv.indexOf(k);
    return i > 0 ? process.argv[i + 1] : undefined;
  };
  const offline = process.argv.includes("--offline");
  const cache = resolve(arg("--cache") ?? join(tmpdir(), "ay-recover"));
  mkdirSync(cache, { recursive: true });
  const snap = async <T>(name: string, produce: () => Promise<T>): Promise<T> => {
    const f = Bun.file(join(cache, `${name}.json`));
    if (offline || (await f.exists() && process.argv.includes("--cached"))) return f.json();
    const v = await produce();
    await Bun.write(f, JSON.stringify(v));
    return v;
  };
  const ay = readAy(root) as ReturnType<typeof readToml>;
  const orgs = Object.values<string>(ay.orgs ?? {});
  const [owner] = [ay.registries?.crates_owner ?? ""];
  const crates = await snap("crates", () => collectCrates(owner));
  const npm = await snap("npm", () => collectNpm(ay.registries?.npm ?? ""));
  const repos = await snap("repos", () => collectRepos(orgs));
  const trees = await snap("trees", () => collectTrees(ay.hosts ?? {}, Object.fromEntries(Object.entries<any>(ay.paths ?? {}).map(([k, v]) => [k, v.work])), ay.recovery?.scan ?? []));
  const legacy = (readToml(privatePath("legacy-inventory.toml", root)).unit ?? []) as any[];

  const manifests: Manifest[] = [];
  const dirMap = new Map<string, string>();
  const archives: Manifest[] = [];
  for (const [key, r] of Object.entries<any>(ay.repos ?? {})) {
    const dir = resolve(root, r.checkout ?? (key === "ay" ? "." : `../${key}`));
    if (!existsSync(join(dir, ".git"))) continue;
    manifests.push(...(await scanCheckout(key, dir)));
    const p = Bun.spawn(["git", "-C", dir, "ls-files"], { stdout: "pipe", stderr: "ignore" });
    dirIndex(key, (await new Response(p.stdout).text()).split("\n").filter(Boolean), dirMap);
  }
  const lrev = (readToml(privatePath("legacy-inventory.toml", root)).meta ?? {}).commit;
  const lco = resolve(root, ay.legacy?.checkout ?? "../legacy");
  if (lrev && existsSync(join(lco, ".git"))) archives.push(...(await scanRevision(`legacy@${lrev.slice(0, 8)}`, lco, lrev)));
  const dirs = (arg("--archive") ?? "").split(",").filter(Boolean).map(d => resolve(d));
  if (!offline) for (const slug of ay.recovery?.archives ?? []) {
    const dir = join(cache, "archives", slug.split("/").pop()!);
    if (!existsSync(dir)) await Bun.$`git clone --quiet --depth 1 https://github.com/${slug} ${dir}`.quiet();
    dirs.push(dir);
  }
  for (const dir of dirs) archives.push(...(await scanCheckout(dir.split(/[\\/]/).pop()!, dir)));
  const ours = new Set(Object.values<any>(ay.repos ?? {}).map(r => r.slug));
  const ctx: Context = { live: new LiveIndex(manifests), archives: new LiveIndex(archives), forks: new Set(repos.filter(r => r.fork && !r.archived && !ours.has(`${r.org}/${r.name}`)).map(r => r.name)), rules: loadRules(), dirs: dirMap };
  const units = assemble({ crates, npm, repos, legacy, trees }, ctx);
  const meta = { scanned: new Date().toISOString().slice(0, 10), owner, orgs: orgs.join(","), manifests: String(ctx.live.size) };
  await Bun.write(resolve(root, arg("--out") ?? privatePath("recovery.toml", root, true)), toToml(units, meta));
  await Bun.write(resolve(root, arg("--md") ?? privatePath("recovery.md", root, true)), toMarkdown(units, meta));
  for (const [o, r] of counts(units)) console.log(`${o.padEnd(7)} live ${r.live} lost ${r.lost} dead ${r.dead}`);
}
