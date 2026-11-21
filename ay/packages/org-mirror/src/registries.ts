/** Registry clients: npm, crates.io and PyPI. Each returns one normalized PackageInfo; parsers are pure so fixtures can test them. */
import { USER_AGENT, type FetchLike } from "./github.ts";

export interface VersionInfo { version: string; publishedAt: string | null; downloads: number | null; yanked: boolean }

export interface PackageInfo {
  status: "ok" | "missing";
  description: string | null;
  latest: string | null;
  homepage: string | null;
  license: string | null;
  /** Source repository URL the registry records, used to check a name really belongs to the repo that declares it. */
  repository: string | null;
  createdAt: string | null;
  lastPublished: string | null;
  downloadsRecent: number | null;
  downloadsWindow: string | null;
  downloadsTotal: number | null;
  url: string;
  versions: VersionInfo[];
}

export const MISSING = (url: string): PackageInfo => ({ status: "missing", description: null, latest: null, homepage: null, license: null, repository: null, createdAt: null, lastPublished: null, downloadsRecent: null, downloadsWindow: null, downloadsTotal: null, url, versions: [] });

export interface RegistryOptions {
  fetch?: FetchLike;
  sleep?: (ms: number) => Promise<void>;
}

const maxDate = (ds: (string | null)[]) => ds.filter((d): d is string => !!d).sort().at(-1) ?? null;
const minDate = (ds: (string | null)[]) => ds.filter((d): d is string => !!d).sort()[0] ?? null;

async function getJson<T>(o: RegistryOptions, url: string, retries = 3): Promise<T | undefined> {
  const doFetch = o.fetch ?? fetch;
  const sleep = o.sleep ?? Bun.sleep;
  for (let i = 0; ; i++) {
    try {
      const res = await doFetch(url, { headers: { accept: "application/json", "user-agent": USER_AGENT }, signal: AbortSignal.timeout(60_000) });
      if (res.status === 404) return undefined;
      if (res.status === 429 || res.status >= 500) throw new Error(`${res.status} ${url}`);
      if (!res.ok) throw new Error(`${res.status} ${url}`);
      return (await res.json()) as T;
    } catch (e) {
      if (i >= retries) throw e;
      await sleep(1500 * 2 ** i);
    }
  }
}

// npm ------------------------------------------------------------------------------------------------------------

interface NpmDoc {
  name: string;
  description?: string;
  license?: string | { type?: string };
  homepage?: string;
  repository?: string | { url?: string };
  "dist-tags"?: { latest?: string };
  time?: Record<string, string>;
  versions?: Record<string, { deprecated?: string }>;
}

const repoUrl = (r: string | { url?: string } | undefined) => (typeof r === "string" ? r : r?.url) ?? null;

export function parseNpm(doc: NpmDoc, downloads: { downloads?: number } | undefined): PackageInfo {
  const time = doc.time ?? {};
  const versions: VersionInfo[] = Object.keys(doc.versions ?? time)
    .filter(v => v !== "created" && v !== "modified")
    .map(v => ({ version: v, publishedAt: time[v] ?? null, downloads: null, yanked: !!doc.versions?.[v]?.deprecated }));
  return {
    status: "ok",
    description: doc.description ?? null,
    latest: doc["dist-tags"]?.latest ?? null,
    homepage: doc.homepage ?? null,
    license: typeof doc.license === "string" ? doc.license : (doc.license?.type ?? null),
    repository: repoUrl(doc.repository),
    createdAt: time.created ?? null,
    lastPublished: maxDate(versions.map(v => v.publishedAt)),
    downloadsRecent: downloads?.downloads ?? null,
    downloadsWindow: downloads ? "30d" : null,
    downloadsTotal: null,
    url: `https://www.npmjs.com/package/${doc.name}`,
    versions,
  };
}

export async function fetchNpm(name: string, o: RegistryOptions = {}): Promise<PackageInfo> {
  const doc = await getJson<NpmDoc>(o, `https://registry.npmjs.org/${name.replace("/", "%2f")}`);
  if (!doc) return MISSING(`https://www.npmjs.com/package/${name}`);
  const downloads = await getJson<{ downloads?: number }>(o, `https://api.npmjs.org/downloads/point/last-month/${name}`).catch(() => undefined);
  return parseNpm(doc, downloads);
}

interface NpmSearch { total: number; objects: { package: { name: string } }[] }

/** Names of the packages published under an npm scope (`@scope/*`): the registry search ranks them first, so paging stops at the first page without one. */
export async function npmScope(scope: string, o: RegistryOptions = {}): Promise<string[]> {
  const names: string[] = [];
  for (let from = 0; from < 1000; from += 250) {
    const r = await getJson<NpmSearch>(o, `https://registry.npmjs.org/-/v1/search?text=${encodeURIComponent(`@${scope}`)}&size=250&from=${from}`);
    if (!r) break;
    const hits = r.objects.map(x => x.package.name).filter(n => n.startsWith(`@${scope}/`));
    names.push(...hits);
    if (!hits.length || r.objects.length < 250) break;
  }
  return [...new Set(names)];
}

// crates.io ------------------------------------------------------------------------------------------------------

interface CratesDoc {
  crate: { id: string; description?: string | null; homepage?: string | null; repository?: string | null; downloads?: number; recent_downloads?: number | null; max_version?: string; newest_version?: string; created_at?: string; updated_at?: string };
  versions: { num: string; created_at: string; downloads: number; yanked: boolean; license?: string | null }[];
}

export function parseCrates(doc: CratesDoc): PackageInfo {
  const versions = doc.versions.map(v => ({ version: v.num, publishedAt: v.created_at, downloads: v.downloads, yanked: v.yanked }));
  const c = doc.crate;
  return {
    status: "ok",
    description: c.description ?? null,
    latest: c.max_version ?? c.newest_version ?? null,
    homepage: c.homepage ?? null,
    license: doc.versions[0]?.license ?? null,
    repository: c.repository ?? null,
    createdAt: c.created_at ?? null,
    lastPublished: maxDate(versions.map(v => v.publishedAt)),
    downloadsRecent: c.recent_downloads ?? null,
    downloadsWindow: c.recent_downloads == null ? null : "90d",
    downloadsTotal: c.downloads ?? null,
    url: `https://crates.io/crates/${c.id}`,
    versions,
  };
}

export async function fetchCrate(name: string, o: RegistryOptions = {}): Promise<PackageInfo> {
  const doc = await getJson<CratesDoc>(o, `https://crates.io/api/v1/crates/${encodeURIComponent(name)}`);
  return doc ? parseCrates(doc) : MISSING(`https://crates.io/crates/${name}`);
}

// PyPI -----------------------------------------------------------------------------------------------------------

interface PypiDoc {
  info: { name: string; summary?: string | null; version?: string; home_page?: string | null; license?: string | null; license_expression?: string | null; project_urls?: Record<string, string> | null };
  releases: Record<string, { upload_time_iso_8601?: string; yanked?: boolean }[]>;
}

export function parsePypi(doc: PypiDoc, stats: { data?: { last_month?: number } } | undefined): PackageInfo {
  const versions = Object.entries(doc.releases).map(([version, files]) => ({
    version,
    publishedAt: minDate(files.map(f => f.upload_time_iso_8601 ?? null)),
    downloads: null,
    yanked: files.length > 0 && files.every(f => f.yanked),
  }));
  const urls = doc.info.project_urls ?? {};
  const repo = Object.values(urls).find(u => /github\.com|gitlab\.com|codeberg\.org/.test(u)) ?? doc.info.home_page ?? null;
  const month = stats?.data?.last_month;
  return {
    status: "ok",
    description: doc.info.summary ?? null,
    latest: doc.info.version ?? null,
    homepage: doc.info.home_page || urls.Homepage || null,
    license: doc.info.license_expression ?? (doc.info.license && doc.info.license.length < 80 ? doc.info.license : null),
    repository: repo,
    createdAt: minDate(versions.map(v => v.publishedAt)),
    lastPublished: maxDate(versions.map(v => v.publishedAt)),
    downloadsRecent: month ?? null,
    downloadsWindow: month === undefined ? null : "30d",
    downloadsTotal: null,
    url: `https://pypi.org/project/${doc.info.name}/`,
    versions,
  };
}

export async function fetchPypi(name: string, o: RegistryOptions = {}): Promise<PackageInfo> {
  const doc = await getJson<PypiDoc>(o, `https://pypi.org/pypi/${encodeURIComponent(name)}/json`);
  if (!doc) return MISSING(`https://pypi.org/project/${name}/`);
  const stats = await getJson<{ data?: { last_month?: number } }>(o, `https://pypistats.org/api/packages/${encodeURIComponent(name.toLowerCase())}/recent`, 0).catch(() => undefined);
  return parsePypi(doc, stats);
}
