#!/usr/bin/env bun
// Follows the upstream evolution of the browser engines: for every upstream of tools/upstreams.toml (pins in
// tools/upstreams.lock.json) and every `[track.*]` entry of testing/acceptance.toml it reports, live, the head of the
// tracked branch, the newest release / channel version, the pin this monorepo is on and how far behind it is.
//
//   bun testing/acceptance/track.ts [--json] [--root <repo>]
//
// GitHub: GraphQL batching when a token is known ($GH_TOKEN, $GITHUB_TOKEN, else `gh auth token`), REST without.
// Every request goes through one pool of 8 with a 20 s timeout; a dead source sets `error` on its entry only.
import { dirname, join, resolve } from "node:path";
import { exists } from "./sys.ts";
import { spawn } from "./sys.ts";
import { merge, readLocal } from "./local.ts";

export type Tracked = {
  name: string;
  kind: string;
  source: string;
  head: string | null;
  head_date: string | null;
  version: string | null;
  version_date: string | null;
  pinned: string | null;
  behind: number | null;
  channels?: Record<string, string>;
  error?: string;
};

export type Checkout = { env?: string; default?: string; ssh?: string };
export type Source = {
  name: string;
  kind: string;
  source: string;
  owner: string | null;
  repo: string | null;
  branch: string;
  pin: string | null; // sha or version this monorepo is on
  pin_file: string | null; // file of the repo holding the pin, read at run time
  pin_deps: string | null; // variable of DEPS holding the pin (gclient vars)
  pin_submodule: string | null; // path of a submodule whose gitlink is the pin
  release: boolean; // follow the newest release or tag of the repo
  provider: string | null; // chromium | firefox | webkit: channel versions from the official release data
  field: string; // what the provider reports: version | v8
  channel: string | null; // channel whose version is `version`
  platform: string | null;
  checkout: Checkout | null; // build checkout whose version file gives `pinned`
};
type Gh = Pick<Tracked, "head" | "head_date" | "version" | "version_date" | "behind">;
type Prov = { version: string | null; version_date: string | null; channels: Record<string, string> };
type Gate = <T>(fn: () => Promise<T>) => Promise<T>;
type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

const TIMEOUT_MS = 20_000;
const POOL = 8;
const CHUNK = 20;
const UA = "browser-acceptance-track";
const isSha = (s: string | null | undefined): s is string => !!s && /^[0-9a-f]{40}$/.test(s);
const iso = (v: unknown): string | null => {
  const d = new Date(v as string | number);
  return v == null || Number.isNaN(d.getTime()) ? null : d.toISOString();
};
const msg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** Bounded concurrency: at most `max` of the wrapped functions run at once, the others wait in order. */
export function limiter(max: number): Gate {
  let active = 0;
  const waiting: (() => void)[] = [];
  return async (fn) => {
    if (active < max) active++;
    else await new Promise<void>((r) => waiting.push(r));
    try {
      return await fn();
    } finally {
      const next = waiting.shift();
      if (next) next();
      else active--;
    }
  };
}

// ---------------------------------------------------------------------------------------------------- manifests

export function repoOf(url: string): { owner: string; repo: string } | null {
  try {
    const u = new URL(url);
    if (u.hostname !== "github.com") return null;
    const [owner, repo] = u.pathname
      .replace(/\.git$/, "")
      .split("/")
      .filter(Boolean);
    return owner && repo ? { owner, repo } : null;
  } catch {
    return null;
  }
}

/** Sources from the text of upstreams.toml, the parsed lock and the `track` table of acceptance.toml. */
export function parseSources(upstreamsToml: string, lock: Json, track: Json): Source[] {
  const out = new Map<string, Source>();
  const base = (name: string, e: Json): Source => {
    const [owner, repo] = String(e.repo ?? "").split("/");
    const gh = e.repo ? { owner, repo } : repoOf(e.url ?? "");
    return {
      name,
      kind: e.kind ?? e.mode ?? "crate",
      source: e.source ?? e.url ?? (gh ? `https://github.com/${gh.owner}/${gh.repo}` : ""),
      owner: gh?.owner ?? null,
      repo: gh?.repo ?? null,
      branch: e.branch ?? "main",
      pin: null,
      pin_file: e.pin_file ?? null,
      pin_deps: e.pin_deps ?? null,
      pin_submodule: e.pin_submodule ?? null,
      release: e.release ?? true,
      provider: e.provider ?? null,
      field: e.field ?? "version",
      channel: e.version_channel ?? null,
      platform: e.platform ?? null,
      checkout: e.checkout ?? null,
    };
  };
  const doc = Bun.TOML.parse(upstreamsToml) as Json;
  for (const u of doc.upstream ?? []) {
    const s = base(u.name, u);
    s.pin = lock?.[u.name]?.sha ?? null;
    out.set(u.name, s);
  }
  for (const [name, e] of Object.entries<Json>(track ?? {})) out.set(name, base(name, e));
  return [...out.values()];
}

async function loadSources(root: string): Promise<{ sources: Source[]; error?: string }> {
  const tools = join(root, "tools");
  const upstreams = await Bun.file(join(tools, "upstreams.toml")).text();
  const lockFile = Bun.file(join(tools, "upstreams.lock.json"));
  const lock = (await lockFile.exists()) ? await lockFile.json() : {};
  let track: Json = {};
  let error: string | undefined;
  try {
    const shared =
      (Bun.TOML.parse(await Bun.file(join(root, "testing", "acceptance.toml")).text()) as Json).track ?? {};
    track = merge(shared, (await readLocal(root)).track ?? {}); // checkouts and hosts are private
  } catch (e) {
    error = `testing/acceptance.toml: ${msg(e)}`;
  }
  const sources = parseSources(upstreams, lock, track);
  for (const s of sources) {
    if (s.pin_submodule) {
      const tree = await Bun.$`git -C ${root} ls-tree HEAD ${s.pin_submodule}`.nothrow().text();
      s.pin = tree.match(/^160000 commit (\w+)/)?.[1] ?? s.pin;
    }
    if (s.pin_deps) {
      const deps = await Bun.file(join(root, "DEPS")).text();
      s.pin = deps.match(new RegExp(`'${s.pin_deps}':\\s*'([^']+)'`))?.[1] ?? s.pin;
    }
    if (!s.pin_file) continue;
    const f = Bun.file(join(root, s.pin_file));
    if (await f.exists()) s.pin = (await f.text()).trim().split(/\s+/)[0] || null;
  }
  return { sources, error };
}

// ------------------------------------------------------------------------------------------------------ transport

async function fetchJson(gate: Gate, url: string, init: RequestInit = {}, text = false): Promise<Json> {
  const res = await gate(() => fetch(url, { ...init, signal: AbortSignal.timeout(TIMEOUT_MS) }));
  if (!res.ok) {
    const hint = res.headers.get("x-ratelimit-remaining") === "0" ? " (rate limit exhausted)" : "";
    throw new Error(`${res.status} ${res.statusText} ${new URL(url).host}${hint}`.trim());
  }
  return text ? res.text() : res.json();
}

async function githubToken(): Promise<string | null> {
  const env = Bun.env.GH_TOKEN || Bun.env.GITHUB_TOKEN;
  if (env) return env;
  try {
    const p = spawn(["gh", "auth", "token"], { stdout: "pipe", stderr: "ignore", timeout: 10_000 });
    const out = (await new Response(p.stdout).text()).trim();
    return (await p.exited) === 0 && out ? out : null;
  } catch {
    return null;
  }
}

// ------------------------------------------------------------------------------------------------ GitHub mapping

const J = (v: string) => JSON.stringify(v);

export function repoQuery(alias: string, s: Source): string {
  const compare = isSha(s.pin) ? `compare(headRef:${J(s.pin)}){behindBy}` : "";
  const release = s.release
    ? `latestRelease{tagName publishedAt} refs(refPrefix:"refs/tags/",first:10,` +
      `orderBy:{field:TAG_COMMIT_DATE,direction:DESC}){nodes{name ` +
      `target{...on Commit{committedDate} ...on Tag{tagger{date}}}}}`
    : "";
  const ref = `ref(qualifiedName:${J(`refs/heads/${s.branch}`)}){target{...on Commit{oid committedDate}} ${compare}}`;
  return `${alias}:repository(owner:${J(s.owner!)},name:${J(s.repo!)}){${ref} ${release}}`;
}

/** One `repository` node of a GraphQL answer, or an Error saying what is missing. */
export function mapRepo(node: Json, branch: string): Gh | Error {
  if (!node) return new Error("repository not found or not readable");
  const commit = node.ref?.target;
  if (!commit?.oid) return new Error(`branch ${branch} not found`);
  // Newest tag that looks like a version: repositories also carry tags such as `rustc-perf` or a bare mirror sha.
  const tag = (node.refs?.nodes ?? []).find((n: Json) => /\d+\.\d+/.test(n.name));
  const rel = node.latestRelease;
  return {
    head: commit.oid,
    head_date: iso(commit.committedDate),
    version: rel?.tagName ?? tag?.name ?? null,
    version_date: iso(rel ? rel.publishedAt : (tag?.target?.committedDate ?? tag?.target?.tagger?.date)),
    behind: typeof node.ref?.compare?.behindBy === "number" ? node.ref.compare.behindBy : null,
  };
}

async function viaGraphql(sources: Source[], token: string, gate: Gate): Promise<Map<string, Gh | Error>> {
  const out = new Map<string, Gh | Error>();
  const chunks: Source[][] = [];
  for (let i = 0; i < sources.length; i += CHUNK) chunks.push(sources.slice(i, i + CHUNK));
  await Promise.all(
    chunks.map(async (chunk) => {
      const query = `query{${chunk.map((s, i) => repoQuery(`r${i}`, s)).join(" ")}}`;
      try {
        const body = await fetchJson(gate, "https://api.github.com/graphql", {
          method: "POST",
          headers: { authorization: `Bearer ${token}`, "user-agent": UA },
          body: JSON.stringify({ query }),
        });
        if (!body.data) throw new Error(body.errors?.[0]?.message ?? "empty GraphQL answer");
        chunk.forEach((s, i) => out.set(s.name, mapRepo(body.data[`r${i}`], s.branch)));
      } catch (e) {
        for (const s of chunk) out.set(s.name, new Error(`graphql: ${msg(e)}`));
      }
    }),
  );
  return out;
}

export function mapRest(commit: Json, compare: Json | null, release: Json | null, tag: Json | null): Gh {
  return {
    head: commit.sha ?? null,
    head_date: iso(commit.commit?.committer?.date),
    version: release?.tag_name ?? tag?.name ?? null,
    version_date: iso(release?.published_at),
    behind: typeof compare?.ahead_by === "number" ? compare.ahead_by : null,
  };
}

async function viaRest(s: Source, token: string | null, gate: Gate): Promise<Gh | Error> {
  const api = `https://api.github.com/repos/${s.owner}/${s.repo}`;
  const headers: Record<string, string> = { accept: "application/vnd.github+json", "user-agent": UA };
  if (token) headers.authorization = `Bearer ${token}`;
  const init = { headers };
  const soft = (url: string) => fetchJson(gate, url, init).catch(() => null);
  try {
    const commit = await fetchJson(gate, `${api}/commits/${encodeURI(s.branch)}`, init);
    const compare = isSha(s.pin) ? await soft(`${api}/compare/${s.pin}...${commit.sha}`) : null;
    let release: Json = null;
    let tag: Json = null;
    if (s.release) {
      release = await soft(`${api}/releases/latest`);
      if (!release) tag = (await soft(`${api}/tags?per_page=1`))?.[0] ?? null;
    }
    return mapRest(commit, compare, release, tag);
  } catch (e) {
    return new Error(`rest: ${msg(e)}`);
  }
}

// ---------------------------------------------------------------------------------------------------- providers

const CHROME_CHANNELS = ["canary", "dev", "beta", "stable"];

export function mapDashRelease(json: Json): { version: string; date: string | null; milestone: number } | null {
  const r = Array.isArray(json) ? json[0] : null;
  return r?.version ? { version: r.version, date: iso(r.time), milestone: r.milestone } : null;
}

/** Chrome for Testing `last-known-good-versions.json`, the fallback when chromiumdash is unreachable. */
export function mapCft(json: Json): Record<string, { version: string; date: string | null }> {
  const out: Record<string, { version: string; date: string | null }> = {};
  for (const [k, v] of Object.entries<Json>(json?.channels ?? {})) {
    out[k.toLowerCase()] = { version: v.version, date: null };
  }
  return out;
}

async function chromiumProvider(s: Source, gate: Gate): Promise<Prov> {
  const platform = s.platform ?? "Linux";
  const dash = "https://chromiumdash.appspot.com";
  const got: Record<string, { version: string; date: string | null }> = {};
  await Promise.all(
    CHROME_CHANNELS.map(async (ch) => {
      const url = `${dash}/fetch_releases?channel=${ch}&platform=${platform}&num=1`;
      const r = mapDashRelease(await fetchJson(gate, url).catch(() => null));
      if (r) got[ch] = { version: r.version, date: r.date };
    }),
  );
  if (!Object.keys(got).length) {
    const cft = "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions.json";
    Object.assign(got, mapCft(await fetchJson(gate, cft)));
  }
  let channels = Object.fromEntries(Object.entries(got).map(([k, v]) => [k, v.version]));
  if (s.field === "v8") {
    const v8: Record<string, string> = {};
    await Promise.all(
      Object.entries(channels).map(async ([ch, version]) => {
        const j = await fetchJson(gate, `${dash}/fetch_version?version=${version}`).catch(() => null);
        if (j?.v8_version) v8[ch] = j.v8_version;
      }),
    );
    channels = v8;
  }
  const lead = s.channel ?? "canary";
  return { version: channels[lead] ?? null, version_date: got[lead]?.date ?? null, channels };
}

/** product-details firefox_versions.json: channel versions of Firefox, hence of SpiderMonkey. */
export function mapFirefox(json: Json, channel: string): Prov {
  const pick = (k: string) => (json?.[k] ? String(json[k]) : null);
  const all: Record<string, string | null> = {
    nightly: pick("FIREFOX_NIGHTLY"),
    beta: pick("FIREFOX_DEVEDITION"),
    stable: pick("LATEST_FIREFOX_VERSION"),
    esr: pick("FIREFOX_ESR"),
  };
  const channels = Object.fromEntries(Object.entries(all).filter(([, v]) => v)) as Record<string, string>;
  // The nightly version changes on the merge day, the stable one on the release day.
  const date = channel === "nightly" ? json?.LAST_MERGE_DATE : channel === "stable" ? json?.LAST_RELEASE_DATE : null;
  return { version: channels[channel] ?? null, version_date: iso(date), channels };
}

/** webkit.org blog feed: newest Safari Technology Preview and newest "WebKit Features for Safari" post. */
export function mapWebkitFeed(xml: string, channel: string): Prov {
  const channels: Record<string, string> = {};
  const dates: Record<string, string | null> = {};
  for (const item of xml.match(/<item>[\s\S]*?<\/item>/g) ?? []) {
    const title = item.match(/<title>([^<]*)<\/title>/)?.[1] ?? "";
    const date = iso(item.match(/<pubDate>([^<]*)<\/pubDate>/)?.[1]);
    // WordPress titles carry non-breaking spaces: \s matches them, a literal space does not.
    const stp = title.match(/Safari\s+Technology\s+Preview\s+(\d+)/);
    const safari = title.match(/^WebKit\s+Features\s+(?:in|for)\s+Safari\s+([\d.]+)/);
    if (stp && !channels.stp) [channels.stp, dates.stp] = [`STP ${stp[1]}`, date];
    if (safari && !channels.safari) [channels.safari, dates.safari] = [safari[1], date];
  }
  return { version: channels[channel] ?? null, version_date: dates[channel] ?? null, channels };
}

async function provide(s: Source, gate: Gate, memo: Map<string, Promise<Prov>>): Promise<Prov | null> {
  if (!s.provider) return null;
  const key = `${s.provider}:${s.field}:${s.platform}:${s.channel}`;
  let p = memo.get(key);
  if (!p) {
    p = (async () => {
      if (s.provider === "chromium") return chromiumProvider(s, gate);
      if (s.provider === "firefox") {
        const json = await fetchJson(gate, "https://product-details.mozilla.org/1.0/firefox_versions.json");
        return mapFirefox(json, s.channel ?? "nightly");
      }
      if (s.provider === "webkit") {
        const xml = await fetchJson(gate, "https://webkit.org/blog/feed/", {}, true);
        return mapWebkitFeed(xml, s.channel ?? "stp");
      }
      throw new Error(`unknown provider ${s.provider}`);
    })();
    memo.set(key, p);
  }
  return p;
}

// ----------------------------------------------------------------------------------------------------- checkout

export function parseChromiumVersion(text: string): string | null {
  const f = (k: string) => text.match(new RegExp(`^${k}=(\\d+)`, "m"))?.[1];
  const parts = ["MAJOR", "MINOR", "BUILD", "PATCH"].map(f);
  return parts.every((p) => p !== undefined) ? parts.join(".") : null;
}

export function parseV8Version(header: string): string | null {
  const f = (k: string) => header.match(new RegExp(`#define V8_${k}\\s+(\\d+)`))?.[1];
  const [major, minor, build, patch] = ["MAJOR_VERSION", "MINOR_VERSION", "BUILD_NUMBER", "PATCH_LEVEL"].map(f);
  if (major === undefined || minor === undefined || build === undefined) return null;
  return `${major}.${minor}.${build}${patch && patch !== "0" ? `.${patch}` : ""}`;
}

/** Reads a file of the Chromium checkout: locally when the directory exists, else over ssh to the build host. */
async function readCheckout(c: Checkout, file: string, gate: Gate): Promise<string | null> {
  const dir = (c.env && Bun.env[c.env]) || c.default;
  if (!dir) return null;
  const local = join(dir, file);
  if (await exists(local)) return Bun.file(local).text();
  if (!c.ssh) return null;
  return gate(async () => {
    const p = spawn(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", c.ssh!, "cat", `${dir}/${file}`], {
      stdout: "pipe",
      stderr: "ignore",
      timeout: TIMEOUT_MS,
    });
    const out = await new Response(p.stdout).text();
    return (await p.exited) === 0 && out ? out : null;
  });
}

async function checkoutPin(s: Source, gate: Gate): Promise<string | null> {
  if (!s.checkout) return null;
  if (s.field === "v8") {
    const h = await readCheckout(s.checkout, "v8/include/v8-version.h", gate);
    return h && parseV8Version(h);
  }
  const v = await readCheckout(s.checkout, "chrome/VERSION", gate);
  return v && parseChromiumVersion(v);
}

// ---------------------------------------------------------------------------------------------------------- main

export async function track(root: string): Promise<Tracked[]> {
  const gate = limiter(POOL);
  const { sources, error: manifestError } = await loadSources(resolve(root));
  const token = await githubToken();
  const github = sources.filter((s) => s.owner && s.repo);
  const gh = token ? await viaGraphql(github, token, gate) : new Map<string, Gh | Error>();
  if (!token) {
    const rest = await Promise.all(github.map((s) => viaRest(s, null, gate)));
    github.forEach((s, i) => gh.set(s.name, rest[i]));
  } else {
    // GraphQL refused as a whole (scope, outage): retry those entries over REST with the same token.
    const failed = github.filter((s) => String((gh.get(s.name) as Error)?.message ?? "").startsWith("graphql:"));
    const rest = await Promise.all(failed.map((s) => viaRest(s, token, gate)));
    failed.forEach((s, i) => gh.set(s.name, rest[i]));
  }
  const memo = new Map<string, Promise<Prov>>();
  const rows = await Promise.all(
    sources.map(async (s): Promise<Tracked> => {
      const t: Tracked = {
        name: s.name,
        kind: s.kind,
        source: s.source,
        head: null,
        head_date: null,
        version: null,
        version_date: null,
        pinned: s.pin,
        behind: null,
      };
      const errors: string[] = [];
      const r = gh.get(s.name);
      if (r instanceof Error) errors.push(r.message);
      else if (r) Object.assign(t, r);
      else if (!s.provider) errors.push("not a github.com repository");
      const [prov, pin] = await Promise.all([
        provide(s, gate, memo).catch((e) => (errors.push(`${s.provider}: ${msg(e)}`), null)),
        checkoutPin(s, gate).catch(() => null),
      ]);
      if (prov) {
        if (s.provider) Object.assign(t, { version: prov.version, version_date: prov.version_date });
        if (Object.keys(prov.channels).length) t.channels = prov.channels;
      }
      if (pin && !t.pinned) t.pinned = pin;
      if (errors.length) t.error = errors.join("; ");
      return t;
    }),
  );
  if (manifestError) {
    rows.push({
      name: "acceptance.toml",
      kind: "manifest",
      source: "",
      head: null,
      head_date: null,
      version: null,
      version_date: null,
      pinned: null,
      behind: null,
      error: manifestError,
    });
  }
  return rows;
}

export function table(rows: Tracked[]): string {
  const short = (v: string | null) => (v && isSha(v) ? v.slice(0, 9) : (v ?? "-"));
  const day = (v: string | null) => v?.slice(0, 10) ?? "-";
  const head = ["name", "kind", "head", "head_date", "version", "version_date", "pinned", "behind"];
  const body = rows.map((r) => [
    r.name,
    r.kind,
    short(r.head),
    day(r.head_date),
    short(r.version),
    day(r.version_date),
    short(r.pinned),
    r.behind == null ? "-" : String(r.behind),
  ]);
  const width = head.map((h, i) => Math.max(h.length, ...body.map((b) => b[i].length)));
  const line = (cells: string[]) =>
    cells
      .map((c, i) => c.padEnd(width[i]))
      .join("  ")
      .trimEnd();
  const out = [line(head), line(width.map((w) => "-".repeat(w)))];
  rows.forEach((r, i) => {
    out.push(line(body[i]));
    if (r.channels)
      out.push(
        `    channels: ${Object.entries(r.channels)
          .map(([k, v]) => `${k}=${v}`)
          .join(" ")}`,
      );
    if (r.error) out.push(`    error: ${r.error}`);
  });
  return out.join("\n");
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const at = args.indexOf("--root");
  const root = at >= 0 ? args[at + 1] : resolve(dirname(import.meta.path), "..", "..");
  const rows = await track(root);
  console.log(args.includes("--json") ? JSON.stringify(rows, null, 2) : table(rows));
}
