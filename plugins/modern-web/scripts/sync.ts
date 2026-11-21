#!/usr/bin/env bun
// Pulls every upstream in sources.ts, regenerates data/*.jsonl, the generated skills and modern-web.lock.json.
// Usage: bun scripts/sync.ts [--offline]   (--offline reuses the cache in $MODERN_WEB_CACHE without fetching)
import { mkdir, rm } from "node:fs/promises";
import { join } from "node:path";
import {
  blob,
  bikeshedSections,
  cacheRoot,
  checkout,
  cite,
  excerpt,
  expandSparse,
  files,
  frontmatter,
  githubJson,
  mapLimit,
  markdownSections,
  pluginRoot,
  rstSections,
  type Entry,
  type Pin,
  type Section,
} from "./lib.ts";
import {
  domainBySpecRepo,
  npmPackages,
  releaseFeeds,
  repos,
  type Domain,
  type RepoKey,
} from "./sources.ts";
import { skillForDomain, writeSkills } from "./skills.ts";

const offline = Bun.argv.includes("--offline");
const started = Date.now();
const log = (message: string) =>
  console.log(`[${((Date.now() - started) / 1000).toFixed(1)}s] ${message}`);

async function pinRepo(repo: string, sparse: string[]): Promise<Pin> {
  if (!offline) return checkout(repo, sparse);
  const dir = join(cacheRoot, repo.replace("/", "__"));
  const [commit, date] = (
    await Bun.$`git -C ${dir} log -1 --format=${"%H %cI"}`.text()
  )
    .trim()
    .split(" ");
  return { repo, commit, date, dir };
}

const pins = Object.fromEntries(
  await mapLimit(Object.entries(repos), 4, async ([key, source]) => {
    const pin = await pinRepo(source.repo, source.sparse);
    log(`${source.repo} ${pin.commit.slice(0, 12)} ${pin.date}`);
    return [key, pin] as const;
  }),
) as Record<RepoKey, Pin>;

const entries: Entry[] = [];
const push = (...items: Entry[]) => entries.push(...items);

// Bun docs and blog: one entry per page, one per section (lint rule pages stay page-level).
function bunDocsUrl(path: string): string {
  const route = path
    .replace(/^docs\//, "")
    .replace(/\.mdx$/, "")
    .replace(/(^|\/)index$/, "");
  if (route.startsWith("blog/"))
    return `https://bun.com/blog/${route.split("/").at(-1)}`;
  return `https://bun.com/docs${route ? `/${route}` : ""}`;
}

async function bunDocs(
  pin: Pin,
  onlyDiffering?: Map<string, string>,
): Promise<{ pages: number; contents: Map<string, string> }> {
  const contents = new Map<string, string>();
  let pages = 0;
  for (const path of await files(pin.dir, "docs/**/*.mdx")) {
    const source = await Bun.file(join(pin.dir, path)).text();
    contents.set(path, source);
    if (onlyDiffering && onlyDiffering.get(path) === source) continue;
    pages++;
    const fork = onlyDiffering !== undefined;
    const { data, body, bodyLine } = frontmatter(source);
    const url = fork ? blob(pin, path) : bunDocsUrl(path);
    const title = data.title || path;
    const kind = path.startsWith("docs/blog/")
      ? "blog"
      : path.startsWith("docs/lint/")
        ? "lint-rule"
        : "docs";
    const tag = fork
      ? onlyDiffering.has(path)
        ? " (fork, changed)"
        : " (fork only)"
      : "";
    const sections = markdownSections(body);
    push({
      domain: "bun",
      kind: `${kind}-page`,
      title: `${title}${tag}`,
      url,
      cite: cite(pin, path, 1),
      text: excerpt(
        `${data.description ?? ""} ${sections[0]?.title ? "" : (sections[0]?.body ?? "")}`,
      ),
    });
    if (kind === "lint-rule") continue;
    for (const section of sections) {
      if (
        !section.title ||
        !section.body.trim() ||
        (kind === "blog" && section.level > 2)
      )
        continue;
      push({
        domain: "bun",
        kind: `${kind}-section`,
        title: `${title} › ${section.title}${tag}`,
        url: `${url}#${section.id}`,
        cite: cite(pin, path, section.line + bodyLine - 1),
        text: excerpt(section.body),
      });
    }
  }
  return { pages, contents };
}

// packages/bun-types: documented declarations with their qualified name (module "bun" is exposed as Bun.*).
function strip(line: string): string {
  return line
    .replace(/(["'`])(?:\\.|(?!\1).)*\1/g, '""')
    .replace(/\/\/.*$/, "");
}

function bunTypes(
  pin: Pin,
  path: string,
  source: string,
  skip?: string,
): Entry[] {
  if (skip === source) return [];
  const found: Entry[] = [];
  const lines = source.split("\n");
  const stack: { name: string; depth: number }[] = [];
  let depth = 0;
  let doc: string[] | undefined;
  let lastDoc = "";
  for (let index = 0; index < lines.length; index++) {
    const raw = lines[index];
    const line = raw.trim();
    if (doc) {
      doc.push(line.replace(/^\*\/?\s?/, ""));
      if (line.includes("*/")) {
        lastDoc = doc.join("\n").replace(/\*\/\s*$/, "");
        doc = undefined;
      }
      continue;
    }
    if (line.startsWith("/**")) {
      if (line.includes("*/")) lastDoc = line.replace(/^\/\*\*|\*\/$/g, "");
      else doc = [line.replace(/^\/\*\*\s?/, "")];
      continue;
    }
    const container = line.match(
      /^(?:export\s+)?(?:declare\s+)?(?:abstract\s+)?(?:namespace|module|interface|class|enum)\s+("[^"]+"|[\w$.]+)/,
    );
    const declaration = line.match(
      /^(?:export\s+)?(?:declare\s+)?(?:function|const|let|var|type)\s+([\w$]+)/,
    );
    const member =
      stack.length && !container && !declaration
        ? line.match(
            /^(?:readonly\s+|static\s+|get\s+|set\s+)*([\w$]+)\??\s*(?:[(<:]|=)/,
          )
        : null;
    const name = container?.[1] ?? declaration?.[1] ?? member?.[1];
    if (name && lastDoc) {
      const scope = stack.map((entry) => entry.name).filter(Boolean);
      const own = name === '"bun"' ? "Bun" : name.replace(/"/g, "");
      const qualified = [...scope, own].join(".");
      const summary = lastDoc
        .split(/\n@/)[0]
        .replace(/\{@link\s+([^}|\s]+)[^}]*\}/g, "$1")
        .trim();
      const examples = lastDoc.match(/@example[\s\S]*/)?.[0] ?? "";
      const callable = member
        ? /^\??\s*[(<]/.test(
            line.slice(line.indexOf(member[1]) + member[1].length),
          )
        : /^(?:export\s+)?(?:declare\s+)?function\b/.test(line);
      found.push({
        domain: "bun",
        kind: "api",
        title: `${qualified}${callable ? "()" : ""}`,
        url: blob(pin, path, index + 1),
        cite: cite(pin, path, index + 1),
        text: excerpt(`${line.replace(/\s*\{$/, "")} — ${summary} ${examples}`),
      });
    }
    if (!line.startsWith("*") && !line.startsWith("//"))
      lastDoc = line ? "" : lastDoc;
    const code = strip(raw);
    const opens = (code.match(/\{/g) ?? []).length;
    const closes = (code.match(/\}/g) ?? []).length;
    if (container && opens > closes) {
      const own =
        container[1] === '"bun"'
          ? "Bun"
          : /^"/.test(container[1])
            ? ""
            : container[1];
      stack.push({ name: own, depth });
    }
    depth += opens - closes;
    while (stack.length && depth <= stack[stack.length - 1].depth) stack.pop();
  }
  return found;
}

async function bunTypeEntries(
  pin: Pin,
  upstream?: Map<string, string>,
): Promise<Map<string, string>> {
  const sources = new Map<string, string>();
  for (const path of await files(pin.dir, "packages/bun-types/*.d.ts")) {
    const source = await Bun.file(join(pin.dir, path)).text();
    sources.set(path, source);
    const items = bunTypes(pin, path, source, upstream?.get(path));
    if (upstream) for (const item of items) item.title += " (fork)";
    push(...items);
  }
  return sources;
}

// The CLI help of the bun on PATH, cited by its revision (the Aphrody fork on the fleet, upstream in CI).
async function bunCli(): Promise<{
  revision: string;
  commands: string[];
  help: Record<string, string>;
}> {
  const revision = (await Bun.$`bun --revision`.text()).trim();
  const main = await Bun.$`bun --help`
    .env({ ...Bun.env, NO_COLOR: "1" })
    .text();
  const help: Record<string, string> = { "": main };
  const commands = [...main.matchAll(/^ {2}([a-z][\w-]*)\s{2,}/gm)].map(
    (match) => match[1],
  );
  for (const command of [...new Set(commands)]) {
    const result = await Bun.$`bun ${command} --help`
      .env({ ...Bun.env, NO_COLOR: "1" })
      .quiet()
      .nothrow();
    const text = `${result.stdout}`.trim();
    if (result.exitCode === 0 && text) help[command] = text;
  }
  for (const [command, text] of Object.entries(help)) {
    push({
      domain: "bun",
      kind: "cli",
      title: `bun ${command || ""}`.trim() + " --help",
      url: command
        ? `https://bun.com/docs/cli/${command}`
        : "https://bun.com/docs/cli",
      cite: `bun ${revision} \`bun ${command ? `${command} ` : ""}--help\``,
      text: excerpt(Bun.stripANSI(text)),
    });
  }
  return { revision, commands: Object.keys(help).filter(Boolean), help };
}

interface Release {
  tag_name: string;
  name: string | null;
  html_url: string;
  published_at: string;
  body: string | null;
  prerelease: boolean;
}

async function releases(): Promise<
  Record<string, { latest: string; date: string; count: number }>
> {
  const summary: Record<
    string,
    { latest: string; date: string; count: number }
  > = {};
  for (const feed of releaseFeeds) {
    const list: Release[] = [];
    for (let page = 1; list.length < feed.limit; page++) {
      const batch = await githubJson<Release[]>(
        `repos/${feed.repo}/releases?per_page=100&page=${page}`,
      );
      list.push(...batch);
      if (batch.length < 100) break;
    }
    const kept = list.slice(0, feed.limit);
    for (const release of kept) {
      push({
        domain: feed.domain,
        kind: "release",
        title: `${feed.repo} ${release.tag_name}${release.prerelease ? " (prerelease)" : ""}`,
        url: release.html_url,
        cite: `${feed.repo} release ${release.tag_name} (${release.published_at})`,
        text: excerpt(release.body ?? release.name ?? ""),
      });
    }
    if (kept[0])
      summary[feed.repo] = {
        latest: kept[0].tag_name,
        date: kept[0].published_at,
        count: kept.length,
      };
  }
  return summary;
}

// Modern Web Guidance: every guide and its sections, under the id `bunx modern-web-guidance retrieve` takes.
interface Guide {
  id: string;
  description: string;
  category: string;
  path: string;
  features: string[];
}

async function guides(pin: Pin): Promise<Guide[]> {
  const found: Guide[] = [];
  for (const path of await files(pin.dir, "guides/**/guide.md")) {
    const source = await Bun.file(join(pin.dir, path)).text();
    const { data, body, bodyLine } = frontmatter(source);
    const id = data.name || path.split("/").at(-2) || path;
    const category = path.split("/")[1];
    const features = (data["web-feature-ids"] ?? "")
      .split(/\s+/)
      .filter(Boolean);
    found.push({
      id,
      description: data.description ?? "",
      category,
      path,
      features,
    });
    push({
      domain: "mwg",
      kind: "guide",
      title: `${id} (${category})`,
      url: blob(pin, path),
      cite: cite(pin, path, 1),
      text: excerpt(
        `${data.description ?? ""} ${features.length ? `web-features: ${features.join(", ")}.` : ""} retrieve: bunx modern-web-guidance@latest retrieve "${id}"`,
      ),
    });
    for (const section of markdownSections(body)) {
      if (!section.title || section.level < 2 || !section.body.trim()) continue;
      push({
        domain: "mwg",
        kind: "guide-section",
        title: `${id} › ${section.title}`,
        url: `${blob(pin, path, section.line + bodyLine - 1)}`,
        cite: cite(pin, path, section.line + bodyLine - 1),
        text: excerpt(section.body),
      });
    }
  }
  for (const path of await files(pin.dir, "skills-src/**/*.md")) {
    const source = await Bun.file(join(pin.dir, path)).text();
    const { body, bodyLine } = frontmatter(source);
    for (const section of markdownSections(body)) {
      if (!section.title || !section.body.trim()) continue;
      push({
        domain: "mwg",
        kind: "skill-section",
        title: `${path.replace(/^skills-src\//, "")} › ${section.title}`,
        url: blob(pin, path, section.line + bodyLine - 1),
        cite: cite(pin, path, section.line + bodyLine - 1),
        text: excerpt(section.body),
      });
    }
  }
  return found;
}

// W3C/WHATWG specs from webref, selected from the sources themselves.
interface WebrefSpec {
  shortname: string;
  title: string;
  shortTitle?: string;
  url: string;
  organization: string;
  standing: string;
  date?: string;
  versions?: string[];
  nightly?: {
    url: string;
    repository?: string;
    sourcePath?: string;
    status?: string;
  };
  release?: { url: string; status?: string };
  headings?: string;
  dfns?: string;
}

interface Heading {
  id: string;
  href: string;
  level: number;
  title: string;
  number?: string;
}

interface Dfn {
  id: string;
  href: string;
  linkingText: string[];
  type: string;
  for: string[];
  access: string;
  informative: boolean;
  heading?: { title: string; number?: string; id?: string };
}

const dfnTypes = new Set([
  "interface",
  "dictionary",
  "method",
  "attribute",
  "dict-member",
  "enum",
  "enum-value",
  "abstract-op",
  "constructor",
  "event",
  "namespace",
  "typedef",
  "const",
  "callback",
  "property",
  "descriptor",
  "function",
  "type",
  "selector",
  "at-rule",
  "element",
  "element-attr",
  "http-header",
  "permission",
]);

function specUrls(spec: WebrefSpec): string[] {
  return [
    spec.url,
    spec.nightly?.url,
    spec.release?.url,
    ...(spec.versions ?? []),
  ].filter((url): url is string => !!url);
}

function selectSpecs(
  all: WebrefSpec[],
  referenced: string[],
): { spec: WebrefSpec; why: string }[] {
  const references = referenced.map((url) => url.split("#")[0]);
  const selected: { spec: WebrefSpec; why: string }[] = [];
  for (const spec of all) {
    if (spec.standing !== "good" || !spec.headings) continue;
    const repository = spec.nightly?.repository ?? "";
    const prefixes = specUrls(spec);
    let why = "";
    if (spec.organization === "WHATWG") why = "WHATWG standard";
    else if (domainBySpecRepo[repository])
      why = `${repository.replace("https://github.com/", "")} spec`;
    else if (repository.startsWith("https://github.com/WebAssembly/"))
      why = "WebAssembly proposal";
    else if (
      references.some((url) =>
        prefixes.some((prefix) => url.startsWith(prefix)),
      )
    )
      why = "referenced by Modern Web Guidance or Bun docs";
    if (why) selected.push({ spec, why });
  }
  return selected;
}

function specDomain(spec: WebrefSpec): Domain {
  const repository = spec.nightly?.repository ?? "";
  if (domainBySpecRepo[repository]) return domainBySpecRepo[repository];
  if (repository.startsWith("https://github.com/WebAssembly/")) return "wasm";
  return "specs";
}

async function webFeatureSpecs(
  features: Set<string>,
): Promise<{ version: string; urls: string[] }> {
  const meta = (await (
    await fetch("https://registry.npmjs.org/web-features/latest")
  ).json()) as { version: string };
  const data = (await (
    await fetch(
      `https://cdn.jsdelivr.net/npm/web-features@${meta.version}/data.json`,
    )
  ).json()) as {
    features: Record<string, { spec?: string | string[] }>;
  };
  const urls: string[] = [];
  for (const id of features) {
    const spec = data.features[id]?.spec;
    if (spec) urls.push(...(Array.isArray(spec) ? spec : [spec]));
  }
  return { version: meta.version, urls };
}

interface Located {
  section: Section;
  path: string;
}

// Sections by id, plus every section in document order for headings whose generated id has no source anchor.
async function sourceSections(
  pin: Pin,
  sourcePath: string,
): Promise<{ byId: Map<string, Located>; ordered: Located[] }> {
  const map = new Map<string, Located>();
  const ordered: Located[] = [];
  const add = (path: string, sections: Section[]) => {
    for (const section of sections) {
      if (section.title) ordered.push({ section, path });
      if (section.id && !map.has(section.id))
        map.set(section.id, { section, path });
    }
  };
  const rstDir = sourcePath.replace(/index\.bs$/, "");
  const rst = await files(pin.dir, `${rstDir}**/*.rst`);
  if (rst.length) {
    for (const path of rst)
      add(path, rstSections(await Bun.file(join(pin.dir, path)).text()));
  }
  const file = Bun.file(join(pin.dir, sourcePath));
  if (await file.exists()) add(sourcePath, bikeshedSections(await file.text()));
  return { byId: map, ordered };
}

const titleKey = (title: string) =>
  title
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();

function matcher(sections: { byId: Map<string, Located>; ordered: Located[] }) {
  const byTitle = new Map<string, Located[]>();
  for (const located of sections.ordered) {
    const key = titleKey(located.section.title);
    byTitle.set(key, [...(byTitle.get(key) ?? []), located]);
  }
  return (heading: Heading): Located | undefined => {
    const direct = sections.byId.get(heading.id);
    if (direct) return direct;
    const candidates = byTitle.get(
      titleKey(heading.title.replace(/^[\d.]+\s+/, "")),
    );
    return candidates?.shift();
  };
}

async function specs(referenced: string[]): Promise<{
  crawl: string;
  selected: {
    shortname: string;
    title: string;
    url: string;
    date?: string;
    why: string;
    sections: number;
    definitions: number;
    excerpts: number;
    source?: string;
  }[];
  sourceRepos: Record<string, { commit: string; date: string }>;
}> {
  const webref = pins.webref;
  const index = (await Bun.file(join(webref.dir, "ed/index.json")).json()) as {
    date: string;
    results: WebrefSpec[];
  };
  const selected = selectSpecs(index.results, referenced);
  await expandSparse(webref, [
    "/ed/index.json",
    ...selected.flatMap(({ spec }) => [
      `/ed/${spec.headings}`,
      ...(spec.dfns ? [`/ed/${spec.dfns}`] : []),
    ]),
  ]);

  // Sparse-clone each spec repository at its source paths; the domain repos are already pinned.
  const pinned: Record<string, Pin> = {
    "https://github.com/gpuweb/gpuweb": pins.gpuweb,
    "https://github.com/WebAssembly/spec": pins.wasmSpec,
  };
  const byRepo = new Map<string, string[]>();
  for (const { spec } of selected) {
    const repository = spec.nightly?.repository;
    if (
      !repository?.startsWith("https://github.com/") ||
      !spec.nightly?.sourcePath ||
      pinned[repository]
    )
      continue;
    byRepo.set(repository, [
      ...(byRepo.get(repository) ?? []),
      `/${spec.nightly.sourcePath}`,
    ]);
  }
  await mapLimit([...byRepo], 6, async ([repository, paths]) => {
    try {
      pinned[repository] = await pinRepo(
        repository.replace("https://github.com/", ""),
        paths,
      );
    } catch (error) {
      log(`skip ${repository}: ${(error as Error).message.split("\n")[0]}`);
    }
  });

  // Proposal forks carry a full copy of the base spec: keep only the headings the base spec does not have.
  const baseTitles = new Set<string>();
  for (const { spec } of selected) {
    if (!/^wasm-(core|js-api|web-api)-\d+$/.test(spec.shortname)) continue;
    const { headings } = (await Bun.file(
      join(webref.dir, "ed", spec.headings!),
    ).json()) as { headings: Heading[] };
    for (const heading of headings) baseTitles.add(titleKey(heading.title));
  }

  const report = [];
  for (const { spec, why } of selected) {
    const domain = specDomain(spec);
    const repository = spec.nightly?.repository ?? "";
    const pin = pinned[repository];
    const find = matcher(
      pin && spec.nightly?.sourcePath
        ? await sourceSections(pin, spec.nightly.sourcePath)
        : { byId: new Map(), ordered: [] },
    );
    const proposal = why === "WebAssembly proposal";
    const { headings } = (await Bun.file(
      join(webref.dir, "ed", spec.headings!),
    ).json()) as { headings: Heading[] };
    const label = spec.shortTitle ?? spec.title;
    const dated = `${spec.nightly?.url ?? spec.url} (${spec.date ?? index.date.slice(0, 10)})`;
    let excerpts = 0;
    let kept = 0;
    for (const heading of headings) {
      if (
        [
          "title",
          "abstract",
          "sotd",
          "toc",
          "status",
          "conformance",
          "references",
          "normative",
          "informative",
          "index",
          "idl-index",
          "issues-index",
        ].includes(heading.id)
      )
        continue;
      if (proposal && baseTitles.has(titleKey(heading.title))) continue;
      const source = find(heading);
      kept++;
      if (source) excerpts++;
      push({
        domain,
        kind: "spec-section",
        title: `${label} § ${heading.number ? `${heading.number} ` : ""}${heading.title}`,
        url: heading.href,
        cite: `${dated}${source ? `; source ${cite(pin!, source.path, source.section.line)}` : ""}`,
        text: source ? excerpt(source.section.body) : "",
      });
    }
    let definitions = 0;
    if (spec.dfns) {
      const { dfns } = (await Bun.file(
        join(webref.dir, "ed", spec.dfns),
      ).json()) as { dfns: Dfn[] };
      for (const dfn of dfns) {
        if (!dfnTypes.has(dfn.type) || dfn.access !== "public") continue;
        definitions++;
        const owner = dfn.for.length ? `${dfn.for[0]}.` : "";
        push({
          domain,
          kind: "spec-definition",
          title: `${label}: ${owner}${dfn.linkingText[0]} (${dfn.type})`,
          url: dfn.href,
          cite: dated,
          text: dfn.heading
            ? `${dfn.type} defined in § ${dfn.heading.number ?? ""} ${dfn.heading.title}${dfn.informative ? " (informative)" : ""}`
            : dfn.type,
        });
      }
    }
    report.push({
      shortname: spec.shortname,
      title: spec.title,
      url: spec.nightly?.url ?? spec.url,
      date: spec.date,
      why,
      sections: kept,
      definitions,
      excerpts,
      source: pin
        ? `${pin.repo}@${pin.commit.slice(0, 12)} ${spec.nightly?.sourcePath}`
        : undefined,
    });
  }
  const sourceRepos = Object.fromEntries(
    Object.values(pinned).map((pin) => [
      pin.repo,
      { commit: pin.commit, date: pin.date },
    ]),
  );
  return { crawl: index.date, selected: report, sourceRepos };
}

// wgpu/Naga: changelog sections, docs pages, documented public items.
async function rustItems(
  pin: Pin,
  pattern: string,
  domain: Domain,
): Promise<number> {
  let count = 0;
  for (const path of await files(pin.dir, pattern)) {
    const lines = (await Bun.file(join(pin.dir, path)).text()).split("\n");
    let doc: string[] = [];
    let owner = "";
    for (let index = 0; index < lines.length; index++) {
      const line = lines[index].trim();
      if (line.startsWith("///")) {
        doc.push(line.replace(/^\/\/\/ ?/, ""));
        continue;
      }
      if (line.startsWith("#[")) continue;
      const impl = line.match(
        /^impl(?:<[^>]*>)?\s+(?:[\w:<>, ]+\s+for\s+)?([A-Z]\w*)/,
      );
      if (impl) owner = impl[1];
      const item = line.match(
        /^pub(?:\([^)]*\))?\s+(?:const\s+|async\s+|unsafe\s+)*(struct|enum|trait|fn|type|const|mod|union)\s+(\w+)/,
      );
      if (item && doc.length) {
        const name =
          item[1] === "fn" && owner && /^\s{4}/.test(lines[index])
            ? `${owner}::${item[2]}`
            : item[2];
        count++;
        push({
          domain,
          kind: "rust-api",
          title: `${pin.repo.split("/")[1]} ${item[1]} ${name}`,
          url: blob(pin, path, index + 1),
          cite: cite(pin, path, index + 1),
          text: excerpt(`${line.replace(/\s*\{$/, "")} — ${doc.join("\n")}`),
        });
      }
      doc = [];
    }
  }
  return count;
}

async function markdownFiles(
  pin: Pin,
  pattern: string,
  domain: Domain,
  kind: string,
  url: (path: string, id: string) => string,
  limit = Infinity,
): Promise<number> {
  let count = 0;
  for (const path of await files(pin.dir, pattern)) {
    const { body, bodyLine } = frontmatter(
      await Bun.file(join(pin.dir, path)).text(),
    );
    const name = path.replace(/\.md$/, "");
    for (const section of markdownSections(body).slice(0, limit)) {
      if (!section.body.trim()) continue;
      count++;
      push({
        domain,
        kind,
        title: `${name}${section.title ? ` › ${section.title}` : ""}`,
        url: url(path, section.id),
        cite: cite(pin, path, section.line + bodyLine - 1),
        text: excerpt(section.body),
      });
    }
  }
  return count;
}

// WASI WIT: interfaces, functions, resources and types with their `///` docs.
async function witItems(pin: Pin): Promise<number> {
  let count = 0;
  for (const path of await files(pin.dir, "proposals/**/*.wit")) {
    const lines = (await Bun.file(join(pin.dir, path)).text()).split("\n");
    let doc: string[] = [];
    let scope = "";
    const pkg =
      lines
        .map((line) => line.match(/^package\s+([^;]+);/)?.[1])
        .find(Boolean) ?? path.split("/")[1];
    for (let index = 0; index < lines.length; index++) {
      const line = lines[index].trim();
      if (line.startsWith("///")) {
        doc.push(line.replace(/^\/\/\/ ?/, ""));
        continue;
      }
      if (line.startsWith("@")) continue;
      const container = line.match(/^(interface|world|resource)\s+([\w-]+)/);
      const item = line.match(
        /^(?:(record|variant|enum|flags|type|use)\s+([\w-]+)|([\w-]+):\s*(?:static\s+)?(func|async func)|(constructor)\()/,
      );
      if (container && /^(interface|world)$/.test(container[1]))
        scope = container[2];
      const name = container
        ? container[2]
        : item
          ? (item[2] ?? item[3] ?? item[5])
          : undefined;
      const type = container?.[1] ?? item?.[1] ?? item?.[4] ?? item?.[5];
      if (name && type !== "use" && (doc.length || container)) {
        count++;
        push({
          domain: "wasm",
          kind: "wit",
          title: `WASI ${pkg} ${container && container[1] !== "resource" ? `${type} ${name}` : `${scope}.${name} (${type})`}`,
          url: blob(pin, path, index + 1),
          cite: cite(pin, path, index + 1),
          text: excerpt(`${line} — ${doc.join("\n")}`),
        });
      }
      doc = [];
    }
  }
  return count;
}

async function npmVersions(): Promise<Record<string, string>> {
  const versions: Record<string, string> = {};
  for (const name of npmPackages) {
    const meta = (await (
      await fetch(`https://registry.npmjs.org/${name}/latest`)
    ).json()) as { version: string };
    versions[name] = meta.version;
  }
  return versions;
}

async function skillVersion(): Promise<string> {
  const [commit] = await githubJson<
    { sha: string; commit: { committer: { date: string } } }[]
  >(
    `repos/${repos.mwg.repo}/commits?path=guides/modern-web-guidance/SKILL.md&per_page=1`,
  );
  return `${commit.commit.committer.date.slice(0, 10).replaceAll("-", "_")}-${commit.sha.slice(0, 8)}`;
}

async function forkStatus(
  base: string,
  head: string,
): Promise<{ ahead: number; behind: number }> {
  const [baseRepo, headRepo] = await Promise.all(
    [base, head].map((repo) =>
      githubJson<{ default_branch: string }>(`repos/${repo}`),
    ),
  );
  const comparison = await githubJson<{ ahead_by: number; behind_by: number }>(
    `repos/${base}/compare/${baseRepo.default_branch}...${head.split("/")[0]}:${headRepo.default_branch}`,
  );
  return { ahead: comparison.ahead_by, behind: comparison.behind_by };
}

// Bun.
const upstreamDocs = await bunDocs(pins.bun);
const forkDocs = await bunDocs(pins.bunFork, upstreamDocs.contents);
const upstreamTypes = await bunTypeEntries(pins.bun);
await bunTypeEntries(pins.bunFork, upstreamTypes);
const cli = await bunCli();
log(
  `bun: ${upstreamDocs.pages} upstream pages, ${forkDocs.pages} fork pages differing, ${cli.commands.length} CLI commands (${cli.revision})`,
);

// Modern Web Guidance and the spec URLs the sources reference.
const guideList = await guides(pins.mwg);
const features = await webFeatureSpecs(
  new Set(guideList.flatMap((guide) => guide.features)),
);
const linkedUrls = [
  ...upstreamDocs.contents.values(),
  ...(await Promise.all(
    guideList.map((guide) => Bun.file(join(pins.mwg.dir, guide.path)).text()),
  )),
].flatMap((text) =>
  [...text.matchAll(/https?:\/\/[^\s)"'<>\]]+/g)].map((match) => match[0]),
);
log(
  `mwg: ${guideList.length} guides, ${features.urls.length} web-features spec URLs`,
);

// WebGPU, wgpu and WebAssembly sources outside webref.
const wgpuApi =
  (await rustItems(pins.wgpu, "wgpu/src/api/**/*.rs", "webgpu")) +
  (await rustItems(pins.wgpu, "wgpu-types/src/**/*.rs", "webgpu"));
const wgpuDocs =
  (await markdownFiles(
    pins.wgpu,
    "CHANGELOG.md",
    "webgpu",
    "changelog",
    (path, id) => `${blob(pins.wgpu, path)}#${id}`,
    400,
  )) +
  (await markdownFiles(
    pins.wgpu,
    "{README.md,docs/**/*.md,naga/README.md}",
    "webgpu",
    "docs",
    (path, id) => `${blob(pins.wgpu, path)}#${id}`,
  ));
const bindgen =
  (await markdownFiles(
    pins.wasmBindgen,
    "guide/src/**/*.md",
    "wasm",
    "wasm-bindgen-guide",
    (path, id) =>
      `https://wasm-bindgen.github.io/wasm-bindgen/${path
        .replace(/^guide\/src\//, "")
        .replace(/(^|\/)README\.md$/, "$1index.md")
        .replace(/\.md$/, ".html")}${id ? `#${id}` : ""}`,
  )) +
  (await markdownFiles(
    pins.wasmBindgen,
    "CHANGELOG.md",
    "wasm",
    "changelog",
    (path, id) => `${blob(pins.wasmBindgen, path)}#${id}`,
    200,
  ));
const wasi =
  (await witItems(pins.wasi)) +
  (await markdownFiles(
    pins.wasi,
    "{README.md,proposals/*/README.md,docs/**/*.md}",
    "wasm",
    "wasi-docs",
    (path, id) => `${blob(pins.wasi, path)}#${id}`,
  ));
log(
  `wgpu: ${wgpuApi} API items, ${wgpuDocs} doc sections; wasm-bindgen ${bindgen}; WASI ${wasi}`,
);

const specReport = await specs([...features.urls, ...linkedUrls]);
log(
  `specs: ${specReport.selected.length} selected from webref crawl ${specReport.crawl}`,
);

const releaseSummary = await releases();
const npm = await npmVersions();
const mwgSkillVersion = await skillVersion();
const forks = {
  "aphrody-labs/bun": await forkStatus(repos.bun.repo, repos.bunFork.repo),
  "aphrody-labs/modern-web-guidance-src": await forkStatus(
    repos.mwg.repo,
    repos.mwgFork.repo,
  ),
};

// Data: one JSONL per domain, sorted and de-duplicated so diffs stay small.
const counts: Record<string, Record<string, number>> = {};
const byDomain = new Map<Domain, Entry[]>();
const seen = new Set<string>();
for (const entry of entries) {
  const key = `${entry.domain}\n${entry.kind}\n${entry.url}\n${entry.title}`;
  if (seen.has(key)) continue;
  seen.add(key);
  byDomain.set(entry.domain, [...(byDomain.get(entry.domain) ?? []), entry]);
  counts[entry.domain] ??= {};
  counts[entry.domain][entry.kind] =
    (counts[entry.domain][entry.kind] ?? 0) + 1;
}
for (const items of byDomain.values())
  items.sort(
    (left, right) =>
      left.kind.localeCompare(right.kind) ||
      left.url.localeCompare(right.url) ||
      left.title.localeCompare(right.title),
  );

const lock = {
  $comment:
    "Generated by scripts/sync.ts. Pinned upstream commits behind data/ and skills/; do not edit.",
  generated: new Date().toISOString(),
  bunCli: cli.revision,
  repos: Object.fromEntries(
    Object.entries(pins).map(([key, pin]) => [
      key,
      { repo: pin.repo, commit: pin.commit, date: pin.date },
    ]),
  ),
  forks,
  specRepos: specReport.sourceRepos,
  webref: { crawl: specReport.crawl, specs: specReport.selected.length },
  npm: { ...npm, "web-features (spec URLs)": features.version },
  modernWebGuidanceSkillVersion: mwgSkillVersion,
  releases: releaseSummary,
  counts,
};
await Bun.write(
  join(pluginRoot, "modern-web.lock.json"),
  `${JSON.stringify(lock, null, 2)}\n`,
);

await writeSkills({
  pins,
  lock,
  entriesByDomain: byDomain,
  cli,
  guides: guideList,
  specs: specReport.selected,
  forkPages: forkDocs.pages,
  upstreamPages: upstreamDocs.pages,
  skillVersion: mwgSkillVersion,
});

// Each skill carries its own index and query script, so it keeps working when copied into another plugin.
await rm(join(pluginRoot, "data"), { recursive: true, force: true });
for (const [domain, items] of byDomain) {
  const dir = join(pluginRoot, "skills", skillForDomain[domain]);
  await mkdir(join(dir, "data"), { recursive: true });
  await Bun.write(
    join(dir, "data", `${domain}.jsonl`),
    `${items.map((item) => JSON.stringify(item)).join("\n")}\n`,
  );
  await Bun.write(
    join(dir, "scripts", "query.ts"),
    Bun.file(join(import.meta.dir, "query.ts")),
  );
}

log(`wrote ${seen.size} entries to skills/*/data and the lock file`);
