/** Package manifests of a repository (package.json, Cargo.toml, pyproject.toml, build.zig.zon, go.mod) and the ecosystem tags they prove. */
import type { Database } from "bun:sqlite";
import { blobIds, lsTree, readBlobs } from "./git.ts";
import { tx } from "./db.ts";

export type Ecosystem = "npm" | "crates" | "pypi" | "zig" | "go";

export interface Manifest {
  path: string;
  ecosystem: Ecosystem;
  name: string | null;
  version: string | null;
  private: boolean;
  /** Evidence of Bun use found in the manifest. */
  bun?: boolean;
}

const FILES: Record<string, Ecosystem> = { "package.json": "npm", "Cargo.toml": "crates", "pyproject.toml": "pypi", "build.zig.zon": "zig", "go.mod": "go" };
const SKIP = /(^|\/)(node_modules|vendor|third_party|fixtures?|testdata|__snapshots__)\//;

/** Manifest paths among a list of repository paths. */
export function manifestPaths(paths: string[]): string[] {
  return paths.filter(p => FILES[p.split("/").pop()!] && !SKIP.test(p));
}

export function parsePackageJson(text: string, path = "package.json"): Manifest | undefined {
  let j: Record<string, unknown>;
  try {
    j = JSON.parse(text);
  } catch {
    return undefined;
  }
  const str = (v: unknown) => (typeof v === "string" ? v : null);
  const scripts = Object.values((j.scripts ?? {}) as Record<string, unknown>).join("\n");
  const deps = { ...(j.dependencies as object), ...(j.devDependencies as object) } as Record<string, unknown>;
  const bun = /^bun@/.test(str(j.packageManager) ?? "") || /(^|[\s;&|])bunx?\s/.test(scripts) || "@types/bun" in deps || "bun-types" in deps || "bun" in ((j.engines as object) ?? {});
  return { path, ecosystem: "npm", name: str(j.name), version: str(j.version), private: j.private === true || !str(j.name), bun };
}

export function parseCargoToml(text: string, path = "Cargo.toml"): Manifest | undefined {
  let t: { package?: { name?: string; version?: unknown; publish?: unknown } };
  try {
    t = Bun.TOML.parse(text) as typeof t;
  } catch {
    return undefined;
  }
  const p = t.package;
  if (!p?.name) return { path, ecosystem: "crates", name: null, version: null, private: true };
  const version = typeof p.version === "string" ? p.version : null;
  return { path, ecosystem: "crates", name: p.name, version, private: p.publish === false || (Array.isArray(p.publish) && p.publish.length === 0) };
}

export function parsePyproject(text: string, path = "pyproject.toml"): Manifest | undefined {
  let t: { project?: { name?: string; version?: string; classifiers?: string[] }; tool?: { poetry?: { name?: string; version?: string }; uv?: { package?: boolean } } };
  try {
    t = Bun.TOML.parse(text) as typeof t;
  } catch {
    return undefined;
  }
  const name = t.project?.name ?? t.tool?.poetry?.name ?? null;
  const version = t.project?.version ?? t.tool?.poetry?.version ?? null;
  const priv = !name || (t.project?.classifiers ?? []).some(c => /Private :: Do Not Upload/i.test(c)) || t.tool?.uv?.package === false;
  return { path, ecosystem: "pypi", name, version, private: priv };
}

/** build.zig.zon is Zig syntax, not data: read the package name (`.name = .foo` or a string) and version with patterns. */
export function parseZon(text: string, path = "build.zig.zon"): Manifest | undefined {
  const head = text.split(/\.dependencies\s*=/)[0]!;
  const name = head.match(/\.name\s*=\s*(?:\.([A-Za-z_]\w*)|@?"([^"]+)")/);
  if (!name) return undefined;
  const version = head.match(/\.version\s*=\s*"([^"]+)"/)?.[1] ?? null;
  return { path, ecosystem: "zig", name: name[1] ?? name[2]!, version, private: false };
}

export function parseGoMod(text: string, path = "go.mod"): Manifest | undefined {
  const m = text.match(/^module\s+(\S+)/m);
  return m ? { path, ecosystem: "go", name: m[1]!, version: null, private: false } : undefined;
}

export function parseManifest(path: string, text: string): Manifest | undefined {
  switch (FILES[path.split("/").pop()!]) {
    case "npm": return parsePackageJson(text, path);
    case "crates": return parseCargoToml(text, path);
    case "pypi": return parsePyproject(text, path);
    case "zig": return parseZon(text, path);
    case "go": return parseGoMod(text, path);
  }
}

/** Language names that become a tag on their own when they hold at least this share of a repository's bytes. */
export const TAG_SHARE = 0.1;
const LANGUAGE_TAG: Record<string, string> = { Rust: "rust", Zig: "zig", Python: "python", TypeScript: "typescript", JavaScript: "javascript", Go: "go", C: "c", "C++": "cpp", Swift: "swift", Kotlin: "kotlin", Java: "java", "C#": "csharp", Lua: "lua" };
/** The tag a language name stands for (rust, zig, python, typescript, go, ...), if it has one. */
export const languageTag = (language: string): string | undefined => LANGUAGE_TAG[language];
const BUN_FILES = new Set(["bun.lock", "bun.lockb", "bunfig.toml"]);
const ECOSYSTEM_TAG: Record<Ecosystem, string> = { npm: "node", crates: "rust", pypi: "python", zig: "zig", go: "go" };

export interface TagEvidence {
  languages: { language: string; share: number }[];
  manifests: Manifest[];
  paths: string[];
}

/** Ecosystem tags (rust, zig, python, bun, typescript, go, ...) with the evidence that produced each. */
export function deriveTags(e: TagEvidence): { tag: string; via: string; share: number | null }[] {
  const tags = new Map<string, { via: string; share: number | null }>();
  for (const l of e.languages) {
    const tag = LANGUAGE_TAG[l.language];
    if (tag && l.share >= TAG_SHARE) tags.set(tag, { via: "language", share: l.share });
  }
  for (const m of e.manifests) {
    const tag = ECOSYSTEM_TAG[m.ecosystem];
    if (tag !== "node" && !tags.has(tag)) tags.set(tag, { via: m.path, share: null });
  }
  if (e.paths.some(p => /(^|\/)build\.zig$/.test(p)) && !tags.has("zig")) tags.set("zig", { via: "build.zig", share: null });
  const bunFile = e.paths.find(p => BUN_FILES.has(p.split("/").pop()!) && !SKIP.test(p));
  const bunManifest = e.manifests.find(m => m.bun);
  if (bunFile || bunManifest) tags.set("bun", { via: bunFile ?? bunManifest!.path, share: null });
  return [...tags].map(([tag, v]) => ({ tag, ...v })).sort((a, b) => a.tag.localeCompare(b.tag));
}

/** Read every manifest at the default branch, store them, and derive the repository's ecosystem tags. */
export async function syncManifests(db: Database, repo: string, dir: string, defaultBranch: string | null): Promise<{ manifests: Manifest[]; tags: string[] }> {
  const rev = defaultBranch ? `refs/heads/${defaultBranch}` : "HEAD";
  const paths = await lsTree(dir, rev).catch(() => [] as string[]);
  const wanted = manifestPaths(paths);
  const ids = await blobIds(dir, wanted.map(p => `${rev}:${p}`));
  const texts = await readBlobs(dir, ids.filter((i): i is string => !!i), 2_000_000);
  const manifests = wanted.flatMap((p, i) => {
    const text = ids[i] ? texts.get(ids[i]!) : null;
    const m = text ? parseManifest(p, text) : undefined;
    return m ? [m] : [];
  });
  const languages = db.query<{ language: string; share: number }, [string]>("SELECT language, share FROM repo_languages WHERE repo = ?").all(repo);
  const tags = deriveTags({ languages, manifests, paths });
  tx(db, () => {
    db.query("DELETE FROM manifests WHERE repo = ?").run(repo);
    const ins = db.query("INSERT OR REPLACE INTO manifests (repo, path, ecosystem, name, version, private) VALUES (?, ?, ?, ?, ?, ?)");
    for (const m of manifests) ins.run(repo, m.path, m.ecosystem, m.name, m.version, +m.private);
    db.query("DELETE FROM repo_tags WHERE repo = ?").run(repo);
    const tag = db.query("INSERT OR REPLACE INTO repo_tags (repo, tag, via, share) VALUES (?, ?, ?, ?)");
    for (const t of tags) tag.run(repo, t.tag, t.via, t.share);
  });
  return { manifests, tags: tags.map(t => t.tag) };
}
