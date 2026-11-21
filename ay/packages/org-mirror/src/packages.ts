/** Packages published from an organization's repos: names come from the manifests and from npm scopes; registries give versions, dates and downloads. */
import type { Database } from "bun:sqlite";
import { tx } from "./db.ts";
import type { Ecosystem } from "./manifests.ts";
import { fetchCrate, fetchNpm, fetchPypi, npmScope, MISSING, type PackageInfo, type RegistryOptions } from "./registries.ts";

export interface Candidate {
  ecosystem: Ecosystem;
  name: string;
  repo: string | null;
  manifestPath: string | null;
  repoCreatedAt?: string | null;
  foundBy: "manifest" | "scope" | "named";
}

const slug = (url: string | null) => url?.replace(/\.git$/, "").replace(/[#?].*$/, "").split("/").filter(Boolean).pop()?.toLowerCase() ?? null;

/**
 * A registry entry belongs to the repo that declares it when it was first published after the repo was created (an older
 * entry is someone else's name) or when the repository it records is the repo's own, under the same owner.
 */
export function belongs(info: PackageInfo, repo: string | null, repoCreatedAt: string | null = null): boolean {
  if (!repo) return true;
  const first = info.versions.map(v => v.publishedAt).filter((d): d is string => !!d).sort()[0];
  const after = !first || !repoCreatedAt || first >= repoCreatedAt;
  if (!info.repository) return after;
  const [owner, name] = repo.toLowerCase().split("/");
  const url = info.repository.toLowerCase().replace(/\.git$/, "");
  if (url.includes(`/${owner}/${name}`)) return true;
  return slug(url) === name && after;
}

/** Every package name the manifests of the org declare (private ones excluded), one candidate per ecosystem and name. */
export function manifestCandidates(db: Database): Candidate[] {
  const rows = db.query<{ repo: string; path: string; ecosystem: Ecosystem; name: string; created_at: string | null }, []>("SELECT m.repo, m.path, m.ecosystem, m.name, r.created_at FROM manifests m LEFT JOIN repos r ON r.full_name = m.repo WHERE m.private = 0 AND m.name IS NOT NULL AND m.ecosystem IN ('npm','crates','pypi','zig') ORDER BY m.repo, m.path").all();
  const seen = new Set<string>();
  const out: Candidate[] = [];
  for (const r of rows) {
    const k = `${r.ecosystem}/${r.name}`;
    if (seen.has(k)) continue;
    seen.add(k);
    out.push({ ecosystem: r.ecosystem, name: r.name, repo: r.repo, manifestPath: r.path, repoCreatedAt: r.created_at, foundBy: "manifest" });
  }
  return out;
}

/** Zig has no registry: a package is a build.zig.zon and its versions are the git tags that look like versions. */
function zigInfo(db: Database, repo: string, version: string | null, url: string): PackageInfo {
  const tags = db.query<{ name: string; date: string | null }, [string]>("SELECT name, date FROM refs WHERE repo = ? AND kind = 'tag'").all(repo).filter(t => /^v?\d+\.\d+/.test(t.name));
  const versions = tags.map(t => ({ version: t.name.replace(/^v/, ""), publishedAt: t.date, downloads: null, yanked: false }));
  return { ...MISSING(url), status: "ok", latest: version, url, versions, lastPublished: versions.map(v => v.publishedAt).filter(Boolean).sort().at(-1) ?? null, createdAt: versions.map(v => v.publishedAt).filter(Boolean).sort()[0] ?? null };
}

function store(db: Database, c: Candidate, info: PackageInfo, status: string, error: string | null): void {
  tx(db, () => {
    db.query(
      `INSERT OR REPLACE INTO packages (ecosystem, name, repo, manifest_path, found_by, status, description, latest, homepage, license, created_at, last_published, versions_count,
         downloads_recent, downloads_window, downloads_total, url, fetched_at, error) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    ).run(c.ecosystem, c.name, c.repo, c.manifestPath, c.foundBy, status, info.description, info.latest, info.homepage, info.license, info.createdAt, info.lastPublished, info.versions.length, info.downloadsRecent, info.downloadsWindow, info.downloadsTotal, info.url, new Date().toISOString(), error);
    db.query("DELETE FROM package_versions WHERE ecosystem = ? AND name = ?").run(c.ecosystem, c.name);
    if (status === "foreign") return;
    const ins = db.query("INSERT OR REPLACE INTO package_versions (ecosystem, name, version, published_at, downloads, yanked) VALUES (?, ?, ?, ?, ?, ?)");
    for (const v of info.versions) ins.run(c.ecosystem, c.name, v.version, v.publishedAt, v.downloads, +v.yanked);
  });
}

export interface PackagesOptions extends RegistryOptions {
  /** npm scopes to search in addition to the scopes of manifest names (the org's own login is always added). */
  scopes?: string[];
  log?: (line: string) => void;
  /** Package names given by hand as `ecosystem:name` (for example a published name that no manifest declares). */
  named?: string[];
  /** Only packages of this repo (full name). */
  repo?: string;
}

/** Run `fn` over `items` with at most `n` in flight. */
async function pool<T>(items: T[], n: number, fn: (item: T) => Promise<void>): Promise<void> {
  const queue = [...items];
  await Promise.all(Array.from({ length: Math.min(n, queue.length) }, async () => {
    for (let c = queue.shift(); c; c = queue.shift()) await fn(c);
  }));
}

export async function syncPackages(db: Database, org: string, o: PackagesOptions = {}): Promise<{ ok: number; missing: number; foreign: number; error: number }> {
  const candidates = manifestCandidates(db).filter(c => !o.repo || c.repo === o.repo);
  const scopes = new Set([org.toLowerCase(), ...(o.scopes ?? []), ...candidates.filter(c => c.ecosystem === "npm" && c.name.startsWith("@")).map(c => c.name.slice(1).split("/")[0]!)]);
  if (!o.repo) {
    for (const scope of scopes) {
      const names = await npmScope(scope, o).catch(() => []);
      for (const name of names) if (!candidates.some(c => c.ecosystem === "npm" && c.name === name)) candidates.push({ ecosystem: "npm", name, repo: null, manifestPath: null, foundBy: "scope" });
    }
  }
  for (const n of o.named ?? []) {
    const [ecosystem, ...rest] = n.split(":");
    const name = rest.join(":");
    if (!["npm", "crates", "pypi"].includes(ecosystem!) || !name) throw new Error(`--package expects npm:NAME, crates:NAME or pypi:NAME, got ${n}`);
    if (!candidates.some(c => c.ecosystem === ecosystem && c.name === name)) candidates.push({ ecosystem: ecosystem as Ecosystem, name, repo: null, manifestPath: null, foundBy: "named" });
  }
  const tally = { ok: 0, missing: 0, foreign: 0, error: 0 };
  const sleep = o.sleep ?? Bun.sleep;
  let crateClock = 0;
  const one = async (c: Candidate) => {
    try {
      let info: PackageInfo;
      if (c.ecosystem === "zig") {
        const v = db.query<{ version: string | null }, [string, string]>("SELECT version FROM manifests WHERE repo = ? AND path = ?").get(c.repo!, c.manifestPath!);
        info = zigInfo(db, c.repo!, v?.version ?? null, `https://github.com/${c.repo}`);
        store(db, c, info, "git", null);
        tally.ok++;
        return;
      }
      if (c.ecosystem === "crates") {
        const wait = crateClock + 1100 - Date.now();
        crateClock = Date.now() + Math.max(0, wait);
        if (wait > 0) await sleep(wait);
      }
      info = c.ecosystem === "npm" ? await fetchNpm(c.name, o) : c.ecosystem === "crates" ? await fetchCrate(c.name, o) : await fetchPypi(c.name, o);
      const status = info.status === "missing" ? "missing" : c.foundBy === "manifest" && !belongs(info, c.repo, c.repoCreatedAt) ? "foreign" : "ok";
      store(db, c, info, status, null);
      tally[status]++;
      o.log?.(`${c.ecosystem} ${c.name}: ${status}${info.versions.length ? ` (${info.versions.length} versions)` : ""}`);
    } catch (e) {
      store(db, c, MISSING(""), "error", String(e).slice(0, 300));
      tally.error++;
    }
  };
  const by = (e: Ecosystem) => candidates.filter(c => c.ecosystem === e);
  await Promise.all([pool(by("npm"), 4, one), pool(by("crates"), 1, one), pool(by("pypi"), 2, one), pool(by("zig"), 1, one)]);
  return tally;
}
