#!/usr/bin/env bun
// Regenerates skills/modern-web (router SKILL.md, references/, data/index.jsonl, scripts/query.ts) and
// scripts/modern-web.lock.json from the upstreams below: Bun upstream and fork, Modern Web Guidance, gpuweb, wgpu,
// WebAssembly, WASI, wasm-bindgen, w3c/webref and the spec repositories webref names. Every entry carries a citation.
// Usage: bun scripts/sync-modern-web.ts [--offline]   (--offline reuses $MODERN_WEB_CACHE without fetching)
import { cp, mkdir, rm } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

// Upstreams the sync pulls. Paths are git sparse-checkout patterns (no-cone); refs default to the remote HEAD.
// Spec selection is not listed here: it is derived from w3c/webref (see selectSpecs in sync.ts).

type Domain = "bun" | "mwg" | "webgpu" | "wasm" | "specs";

interface RepoSource {
  repo: string;
  sparse: string[];
  role: string;
}

const repos = {
  bun: {
    repo: "oven-sh/bun",
    sparse: ["/docs/**", "/packages/bun-types/**"],
    role: "Bun upstream: docs, blog and release notes (docs/blog), runtime API types",
  },
  bunFork: {
    repo: "aphrody-labs/bun",
    sparse: ["/docs/**", "/packages/bun-types/**"],
    role: "Aphrody runtime fork: pages and API types that differ from upstream",
  },
  mwg: {
    repo: "GoogleChrome/modern-web-guidance-src",
    sparse: ["/guides/**", "/skills-src/**", "/package.json"],
    role: "Modern Web Guidance guides and standalone skills",
  },
  mwgFork: {
    repo: "aphrody-labs/modern-web-guidance-src",
    sparse: ["/package.json", "/scripts/**"],
    role: "Bun-migrated fork of modern-web-guidance-src",
  },
  gpuweb: {
    repo: "gpuweb/gpuweb",
    sparse: ["/spec/index.bs", "/wgsl/index.bs"],
    role: "WebGPU and WGSL Bikeshed sources",
  },
  wgpu: {
    repo: "gfx-rs/wgpu",
    sparse: [
      "/CHANGELOG.md",
      "/README.md",
      "/docs/**",
      "/naga/README.md",
      "/wgpu/src/api/**",
      "/wgpu-types/src/**",
    ],
    role: "wgpu and Naga: changelog, docs, public API doc comments",
  },
  wasmSpec: {
    repo: "WebAssembly/spec",
    sparse: [
      "/document/core/**",
      "/document/js-api/index.bs",
      "/document/web-api/index.bs",
    ],
    role: "WebAssembly core (RST), JS API and Web API (Bikeshed) sources",
  },
  wasi: {
    repo: "WebAssembly/WASI",
    sparse: ["/README.md", "/proposals/**", "/docs/**"],
    role: "WASI proposals and WIT interfaces",
  },
  wasmBindgen: {
    repo: "wasm-bindgen/wasm-bindgen",
    sparse: ["/guide/src/**", "/CHANGELOG.md"],
    role: "wasm-bindgen guide (mdBook) and changelog",
  },
  webref: {
    repo: "w3c/webref",
    sparse: ["/ed/index.json"],
    role: "W3C webref crawl: spec metadata, headings and definitions",
  },
} satisfies Record<string, RepoSource>;

type RepoKey = keyof typeof repos;

// Spec repositories whose sections belong to a dedicated domain rather than the generic web-specs skill.
const domainBySpecRepo: Record<string, Domain> = {
  "https://github.com/gpuweb/gpuweb": "webgpu",
  "https://github.com/WebAssembly/spec": "wasm",
};

const releaseFeeds = [
  { repo: "oven-sh/bun", domain: "bun" as Domain, limit: 400 },
  { repo: "aphrody-labs/bun", domain: "bun" as Domain, limit: 60 },
  { repo: "gfx-rs/wgpu", domain: "webgpu" as Domain, limit: 40 },
  { repo: "wasm-bindgen/wasm-bindgen", domain: "wasm" as Domain, limit: 40 },
];

const npmPackages = ["modern-web-guidance", "web-features"];

interface Entry {
  domain: Domain;
  kind: string;
  title: string;
  url: string;
  cite: string;
  text: string;
}

interface Pin {
  repo: string;
  commit: string;
  date: string;
  dir: string;
}

const pluginRoot = join(import.meta.dir, "..");
const cacheRoot =
  Bun.env.MODERN_WEB_CACHE ?? join(homedir(), ".cache", "modern-web");
const excerptLength = 280;

// Network failures (connection resets under many parallel clones) are retried with a growing delay.
async function git(args: string[], cwd?: string, attempt = 1): Promise<string> {
  const process = Bun.spawn(["git", ...args], {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    process.exited,
  ]);
  if (
    code !== 0 &&
    attempt < 4 &&
    /unable to access|Connection|timed out|early EOF|RPC failed/i.test(stderr)
  ) {
    await Bun.sleep(2000 * attempt);
    return git(args, cwd, attempt + 1);
  }
  if (code !== 0)
    throw new Error(`git ${args.join(" ")} (${cwd ?? "."}): ${stderr.trim()}`);
  return stdout.trim();
}

// Shallow, blobless, sparse checkout of the remote HEAD into the private cache; re-runs fetch and move to the new HEAD.
async function checkout(repo: string, sparse: string[]): Promise<Pin> {
  const dir = join(cacheRoot, repo.replace("/", "__"));
  const url = `https://github.com/${repo}.git`;
  if (!(await Bun.file(join(dir, ".git", "HEAD")).exists())) {
    await git([
      "clone",
      "--quiet",
      "--depth",
      "1",
      "--filter=blob:none",
      "--sparse",
      "--no-checkout",
      url,
      dir,
    ]);
    await git(["config", "core.longpaths", "true"], dir);
  } else {
    await git(
      [
        "fetch",
        "--quiet",
        "--depth",
        "1",
        "--filter=blob:none",
        "origin",
        "HEAD",
      ],
      dir,
    );
  }
  await git(["sparse-checkout", "set", "--no-cone", ...sparse], dir);
  const target = (await Bun.file(join(dir, ".git", "FETCH_HEAD")).exists())
    ? "FETCH_HEAD"
    : "HEAD";
  await git(["checkout", "--quiet", "--force", "--detach", target], dir);
  const [commit, date] = (
    await git(["log", "-1", "--format=%H %cI"], dir)
  ).split(" ");
  return { repo, commit, date, dir };
}

async function expandSparse(pin: Pin, sparse: string[]): Promise<void> {
  await git(["sparse-checkout", "set", "--no-cone", ...sparse], pin.dir);
}

function blob(pin: Pin, path: string, line?: number): string {
  return `https://github.com/${pin.repo}/blob/${pin.commit}/${path}${line ? `#L${line}` : ""}`;
}

function cite(pin: Pin, path: string, line?: number): string {
  return `${pin.repo}@${pin.commit.slice(0, 12)} ${path}${line ? `:${line}` : ""}`;
}

async function files(dir: string, pattern: string): Promise<string[]> {
  const found: string[] = [];
  for await (const path of new Bun.Glob(pattern).scan({
    cwd: dir,
    onlyFiles: true,
  })) {
    found.push(path.replaceAll("\\", "/"));
  }
  return found.sort();
}

const entities: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
  mdash: "—",
  ndash: "–",
  hellip: "…",
  times: "×",
  rarr: "→",
  larr: "←",
  le: "≤",
  ge: "≥",
  ne: "≠",
  lsquo: "‘",
  rsquo: "’",
  ldquo: "“",
  rdquo: "”",
  middot: "·",
};

function linkText(inner: string): string {
  const display = inner.split("|");
  if (display.length > 1) return display.at(-1) ?? inner;
  return inner.split("/").at(-1) || inner;
}

// Turns Bikeshed, Wattsi, RST, MDX or Markdown source into plain prose for excerpts and full-text search.
// Markdown code (fenced or inline) keeps its angle brackets: tags are stripped only from prose.
const openMark = "\u{E000}";
const closeMark = "\u{E001}";
const protectCode = (source: string) =>
  source.replace(/```[\s\S]*?```|`[^`\n]+`/g, (code) =>
    code.replaceAll("<", openMark).replaceAll(">", closeMark),
  );

function plain(source: string): string {
  return protectCode(source)
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/^\s*(import|export) .*$/gm, " ")
    .replace(/\{\{([^}]+?)\}\}/g, (_, inner: string) => {
      const parts = inner.split("|");
      const target = parts[0].replace(/^"|"$/g, "");
      return parts[1] ?? target.replace(/\//g, ".");
    })
    .replace(/\[=([^\]]+?)=\]/g, (_, inner: string) => linkText(inner))
    .replace(/\[\$([^\]]+?)\$\]/g, (_, inner: string) => linkText(inner))
    .replace(/\[\[!?([^\]]+?)\]\]/g, "[$1]")
    .replace(/<<([^>]+?)>>/g, "<$1>")
    .replace(/'([a-z-]+)'/g, "$1")
    .replace(/\|([A-Za-z][\w ]*?)\|/g, "$1")
    .replace(/:(?:ref|math|token|code|term|dfn):`([^`<]*?)(?:<[^>]*>)?`/g, "$1")
    .replace(/\$\$\{[^}]*\}/g, " ")
    .replace(/<\/?[A-Za-z][^>]*>/g, " ")
    .replace(/&(#x?[0-9a-f]+|[a-z]+);/gi, (match, name: string) => {
      if (name.startsWith("#")) {
        const hex = /^#x/i.test(name);
        const code = Number.parseInt(name.slice(hex ? 2 : 1), hex ? 16 : 10);
        return Number.isInteger(code) && code >= 0 && code <= 0x10ffff
          ? String.fromCodePoint(code)
          : match;
      }
      return entities[name.toLowerCase()] ?? match;
    })
    .replace(/^\.\. [^\n]*$/gm, " ")
    .replace(/^\s*[=\-~^"'`#*+]{3,}\s*$/gm, " ")
    .replace(/\s+/g, " ")
    .replaceAll(openMark, "<")
    .replaceAll(closeMark, ">")
    .trim();
}

function excerpt(source: string): string {
  const text = plain(source);
  if (text.length <= excerptLength) return text;
  const cut = text.lastIndexOf(" ", excerptLength);
  return `${text.slice(0, cut > 0 ? cut : excerptLength)} …`;
}

interface Section {
  id: string;
  title: string;
  level: number;
  line: number;
  body: string;
}

// Bikeshed markdown headings (`## Title ## {#id}`), HTML headings with ids (Wattsi, .src.html, inline in .bs).
function bikeshedSections(source: string): Section[] {
  const lines = source.split("\n");
  const sections: Section[] = [];
  let current: Section | undefined;
  const push = () => {
    if (current) sections.push(current);
  };
  // ReSpec and ecmarkup put the id on the enclosing <section>/<emu-clause>, not on the heading.
  let containerId = "";
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    const container = line.match(
      /<(?:section|emu-clause|emu-annex)\b[^>]*\bid=["']?([^"'\s>]+)/i,
    );
    if (container) containerId = container[1];
    const markdown = line.match(
      /^(#{1,6})\s+(.+?)\s*#*\s*(?:\{#([^}\s]+)[^}]*\})?\s*$/,
    );
    const html = markdown
      ? null
      : line.match(/<h([1-6])\b([^>]*)>(.*?)(?:<\/h\1>|$)/i);
    if (markdown || html) {
      push();
      const id = markdown
        ? (markdown[3] ?? "")
        : (html![2].match(/\bid=["']?([^"'\s>]+)/)?.[1] ?? containerId);
      containerId = "";
      current = markdown
        ? {
            id,
            title: plain(markdown[2]),
            level: markdown[1].length,
            line: index + 1,
            body: "",
          }
        : {
            id,
            title: plain(html![3]),
            level: Number(html![1]),
            line: index + 1,
            body: "",
          };
      continue;
    }
    if (current && current.body.length < 8000) current.body += `${line}\n`;
  }
  push();
  return sections;
}

// Sphinx RST: `.. _label:` immediately before a title with an underline.
function rstSections(source: string): Section[] {
  const lines = source.split("\n");
  const sections: Section[] = [];
  let pendingLabels: string[] = [];
  let current: Section | undefined;
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    const label = line.match(/^\.\. _([^:]+):\s*$/);
    if (label) {
      pendingLabels.push(label[1]);
      continue;
    }
    const next = lines[index + 1] ?? "";
    if (
      line.trim() &&
      /^([=\-~^"'`#*+])\1{2,}\s*$/.test(next) &&
      next.trim().length >= line.trim().length - 1
    ) {
      if (current) sections.push(current);
      const level = "=-~^\"'`#*+".indexOf(next[0]) + 1;
      current = {
        id: pendingLabels[0] ?? "",
        title: plain(line),
        level,
        line: index + 1,
        body: "",
      };
      for (const alias of pendingLabels.slice(1))
        sections.push({ ...current, id: alias });
      pendingLabels = [];
      index++;
      continue;
    }
    if (line.trim() && !line.startsWith(".. ") && !line.startsWith("   "))
      pendingLabels = [];
    if (current && current.body.length < 8000) current.body += `${line}\n`;
  }
  if (current) sections.push(current);
  return sections;
}

// Markdown/MDX headings (`#`..`######`) outside fenced code.
function markdownSections(source: string): Section[] {
  const lines = source.split("\n");
  const sections: Section[] = [];
  let current: Section = { id: "", title: "", level: 0, line: 1, body: "" };
  let fenced = false;
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    if (/^\s*(```|~~~)/.test(line)) fenced = !fenced;
    const heading = fenced ? null : line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/);
    if (heading) {
      sections.push(current);
      const title = plain(heading[2]);
      current = {
        id: slug(title),
        title,
        level: heading[1].length,
        line: index + 1,
        body: "",
      };
      continue;
    }
    if (current.body.length < 8000) current.body += `${line}\n`;
  }
  sections.push(current);
  return sections.filter((section) => section.title || section.body.trim());
}

function slug(title: string): string {
  return title
    .toLowerCase()
    .replace(/[`*_~]/g, "")
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .trim()
    .replace(/\s+/g, "-");
}

function frontmatter(source: string): {
  data: Record<string, string>;
  body: string;
  bodyLine: number;
} {
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?/);
  if (!match) return { data: {}, body: source, bodyLine: 1 };
  const data: Record<string, string> = {};
  let key = "";
  for (const line of match[1].split(/\r?\n/)) {
    const pair = line.match(/^([\w-]+):\s*(.*)$/);
    if (pair) {
      key = pair[1];
      data[key] = pair[2].replace(/^["']|["']$/g, "").replace(/^\|$/, "");
    } else if (key && line.trim()) {
      data[key] = `${data[key]} ${line.trim().replace(/^- /, "")}`.trim();
    }
  }
  return {
    data,
    body: source.slice(match[0].length),
    bodyLine: match[0].split("\n").length,
  };
}

async function githubToken(): Promise<string | undefined> {
  const fromEnv = Bun.env.GH_TOKEN ?? Bun.env.GITHUB_TOKEN;
  if (fromEnv) return fromEnv;
  const result = await Bun.$`gh auth token`.quiet().nothrow();
  return result.exitCode === 0 ? result.text().trim() : undefined;
}

async function githubJson<T>(path: string): Promise<T> {
  const token = await githubToken();
  const response = await fetch(`https://api.github.com/${path}`, {
    headers: {
      accept: "application/vnd.github+json",
      "user-agent": "modern-web-sync",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
  });
  if (!response.ok)
    throw new Error(`GET ${path}: ${response.status} ${await response.text()}`);
  return (await response.json()) as T;
}

async function mapLimit<T, R>(
  items: T[],
  limit: number,
  worker: (item: T) => Promise<R>,
): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (next < items.length) {
        const index = next++;
        results[index] = await worker(items[index]);
      }
    }),
  );
  return results;
}

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
    const dated = `${spec.shortname} (${spec.date ?? index.date.slice(0, 10)})`;
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

// ---- output: one router skill, references on demand, one compact index -----------------------------------------
const skillDir = join(pluginRoot, "skills", "modern-web");
const lockPath = join(import.meta.dir, "modern-web.lock.json");
const generatedAt = new Date().toISOString();
const banner = `<!-- Generated by plugins/aphrody/scripts/sync-modern-web.ts on ${generatedAt}; do not edit. -->`;

// Dedupe by key, then drop repeated excerpts so no text is stored twice across sources.
const counts: Record<string, Record<string, number>> = {};
const kept: Entry[] = [];
const seen = new Set<string>();
const texts = new Set<string>();
for (const entry of entries) {
  const key = `${entry.domain}\n${entry.kind}\n${entry.url}\n${entry.title}`;
  if (seen.has(key)) continue;
  seen.add(key);
  if (
    entry.kind !== "spec-definition" &&
    entry.text.length > 40 &&
    texts.has(entry.text)
  )
    entry.text = "";
  else texts.add(entry.text);
  kept.push(entry);
  counts[entry.domain] ??= {};
  counts[entry.domain][entry.kind] =
    (counts[entry.domain][entry.kind] ?? 0) + 1;
}
const domainOrder: Domain[] = ["bun", "mwg", "webgpu", "wasm", "specs"];
kept.sort(
  (left, right) =>
    domainOrder.indexOf(left.domain) - domainOrder.indexOf(right.domain) ||
    left.kind.localeCompare(right.kind) ||
    left.url.localeCompare(right.url) ||
    left.title.localeCompare(right.title),
);

// Compact rows: d(omain) k(ind) t(itle) c(ite) x (excerpt, omitted when empty) u (URL, omitted when it is the
// GitHub blob URL that query.ts rebuilds from the citation).
const derivable = (entry: Entry) => {
  const source = entry.cite.match(
    /^([\w.-]+\/[\w.-]+)@([0-9a-f]{12}) (\S+?)(?::(\d+))?$/,
  );
  return (
    !!source &&
    entry.url.startsWith(`https://github.com/${source[1]}/blob/${source[2]}`)
  );
};
// Spec rows cite `shortname` and store their URL relative to the spec draft (`~anchor`); the header row holds the
// draft URL and date of every spec once instead of on each of their sections and definitions.
const specMeta = Object.fromEntries(
  specReport.selected.map((row) => [
    row.shortname,
    { u: row.url, date: row.date ?? "" },
  ]),
);
const rows = [
  JSON.stringify({ h: 1, specs: specMeta }),
  ...kept.map((entry) => {
    const spec = entry.cite.match(/^([\w.-]+) \(([^)]*)\)(.*)$/);
    const meta = spec ? specMeta[spec[1]] : undefined;
    const cite = meta ? `${spec![1]}${spec![3]}` : entry.cite;
    const url =
      meta && entry.url.startsWith(meta.u)
        ? `~${entry.url.slice(meta.u.length)}`
        : entry.url;
    const text =
      entry.kind === "spec-definition"
        ? entry.text.replace(/ defined in § ([\d.A-Z]*) .*$/, " § $1")
        : entry.text;
    return JSON.stringify({
      d: entry.domain,
      k: entry.kind,
      t: entry.title,
      c: cite,
      ...(derivable(entry) ? {} : { u: url }),
      ...(text ? { x: text } : {}),
    });
  }),
];

const lock = {
  $comment:
    "Generated by sync-modern-web.ts: pinned upstreams behind skills/modern-web; do not edit.",
  generated: generatedAt,
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

const escape = (text: string) =>
  text.replace(/\|/g, "\\|").replace(/\s+/g, " ").trim();
const pinLine = (key: RepoKey) =>
  `[${pins[key].repo}@${pins[key].commit.slice(0, 12)}](https://github.com/${pins[key].repo}/tree/${pins[key].commit}) (${pins[key].date.slice(0, 10)})`;
const byUrlEnd = (suffix: string) =>
  kept.find((entry) => entry.url.endsWith(suffix));
const citeFor = (suffix: string) => {
  const entry = byUrlEnd(suffix);
  return entry ? `[${entry.title}](${entry.url})` : "";
};
const of = (domain: Domain, kind?: string) =>
  kept.filter(
    (entry) => entry.domain === domain && (!kind || entry.kind === kind),
  );
const specRows = (rows: typeof specReport.selected) =>
  [
    "| Spec | Draft | Date | Sections (excerpts) | Defs |",
    "| --- | --- | --- | --- | --- |",
    ...rows.map(
      (row) =>
        `| \`${row.shortname}\` ${escape(row.title)} | ${row.url} | ${row.date ?? ""} | ${row.sections} (${row.excerpts}) | ${row.definitions} |`,
    ),
  ].join("\n");

// references/bun.md
const bunPages = of("bun").filter((entry) => entry.kind.endsWith("-page"));
const docDirs = new Map<string, number>();
for (const page of bunPages) {
  const parts = (page.cite.split(" ")[1] ?? "").split("/");
  const dir = parts.length > 2 ? parts[1] : "(root)";
  docDirs.set(dir, (docDirs.get(dir) ?? 0) + 1);
}
const bunFork = forks["aphrody-labs/bun"];
const bunMd = `# Bun (upstream + Aphrody fork)

${banner}

Sources: ${pinLine("bun")}, fork ${pinLine("bunFork")} (${bunFork.ahead} ahead, ${bunFork.behind} behind upstream; ${forkDocs.pages} fork docs pages differ and are indexed with a \`(fork ...)\` tag). CLI help from \`bun ${cli.revision}\`.
Releases: ${Object.entries(releaseSummary)
  .filter(([repo]) => repo.endsWith("/bun"))
  .map(
    ([repo, release]) =>
      `${repo} \`${release.latest}\` (${release.date.slice(0, 10)})`,
  )
  .join(", ")}.

Query kinds (\`--domain bun --kind <k>\`): ${Object.entries(counts.bun ?? {})
  .map(([kind, count]) => `\`${kind}\` ${count}`)
  .join(", ")}.

Docs pages per directory: ${[...docDirs]
  .sort((left, right) => right[1] - left[1])
  .map(([dir, count]) => `${dir} ${count}`)
  .join(", ")}.

CLI commands: ${cli.commands.map((command) => `\`${command}\``).join(" ")}.

Top-level \`Bun.*\` declarations: ${[
  ...new Set(
    of("bun", "api")
      .map((entry) => entry.title)
      .filter((title) => /^Bun\.[\w$]+(\(\))?$/.test(title)),
  ),
]
  .map((title) => `\`${title}\``)
  .join(" ")}.
`;

// references/guidance.md: the upstream SKILL.md workflow with bunx, then the guide ids.
const mwgPin = pins.mwg;
const upstreamSkill = frontmatter(
  await Bun.file(
    join(mwgPin.dir, "guides/modern-web-guidance/SKILL.md"),
  ).text(),
)
  .body.replaceAll("SKILL_VERSION", mwgSkillVersion)
  .replace(
    /npx -y modern-web-guidance@latest/g,
    "bunx modern-web-guidance@latest",
  )
  .replace(
    /pnpx modern-web-guidance@latest/g,
    "bunx modern-web-guidance@latest",
  )
  .replace(/directly with `npx`/g, "directly with `bunx`")
  .replace(/^# Modern Web Guidance\n/m, "")
  .replace(
    /## Using npx \/ pnpx[\s\S]*?(?=\n## )/,
    '## Using bunx\n\n- Run `bunx modern-web-guidance@latest`; never npx, pnpx or yarn dlx. Allowlist `bunx modern-web-guidance@latest *`, never bare `bunx *`. It needs network access to the npm registry.\n- Offline: `bun scripts/query.ts "<query>" --domain mwg` searches the pinned guides.\n',
  )
  .trim();
const categories = new Map<string, string[]>();
for (const guide of guideList)
  categories.set(guide.category, [
    ...(categories.get(guide.category) ?? []),
    guide.id,
  ]);
const guidanceMd = `# Modern Web Guidance

${banner}

Source: ${pinLine("mwg")}, skill version \`${mwgSkillVersion}\`, npm \`modern-web-guidance@${npm["modern-web-guidance"]}\`; Bun-migrated fork ${pinLine("mwgFork")}.

${upstreamSkill}

## Guide ids (\`retrieve\` argument), by category

${[...categories]
  .sort()
  .map(([category, ids]) => `- ${category}: ${ids.sort().join(", ")}`)
  .join("\n")}
`;

// references/webgpu.md: the practice Aphrody measured, each point tied to the normative section.
const webgpuRows = specReport.selected.filter((row) =>
  row.url.includes("gpuweb.github.io"),
);
const chapters = (label: string) =>
  of("webgpu", "spec-section")
    .filter((entry) => new RegExp(`^${label} § \\d+ `).test(entry.title))
    .map((entry) => `[${entry.title.replace(`${label} § `, "")}](${entry.url})`)
    .join(" · ");
const wgpuRelease = releaseSummary["gfx-rs/wgpu"];
const webgpuMd = `# WebGPU, WGSL, wgpu, Naga

${banner}

Sources: ${pinLine("gpuweb")}, ${pinLine("wgpu")}${wgpuRelease ? ` (latest release \`${wgpuRelease.latest}\`, ${wgpuRelease.date.slice(0, 10)})` : ""}, webref ${pinLine("webref")}.

## Aphrody practice (measured)

1. Configure the surface once: one \`GPUCanvasContext.configure()\` with the device and \`navigator.gpu.getPreferredCanvasFormat()\`; a resize only changes \`canvas.width\`/\`height\`, which replaces the drawing buffer without reconfiguring. ${citeFor("#dom-gpucanvascontext-configure")} ${citeFor("#context-sizing")}
2. Size from \`ResizeObserver\` \`devicePixelContentBoxSize\`, clamped to \`device.limits.maxTextureDimension2D\` (a larger canvas fails validation in \`getCurrentTexture()\`). ${citeFor("#dom-resizeobserverboxoptions-device-pixel-content-box")} ${citeFor("#dom-supported-limits-maxtexturedimension2d")}
3. Render on demand: one \`requestAnimationFrame\` when state or size changes, then \`getCurrentTexture()\` for that frame; no free-running loop. ${citeFor("#dom-animationframeprovider-requestanimationframe")} ${citeFor("#dom-gpucanvascontext-getcurrenttexture")}
4. No WebGL fallback: when \`navigator.gpu\` is missing or \`requestAdapter()\` resolves \`null\`, say WebGPU is unavailable. ${citeFor("#dom-gpu-requestadapter")}

## Specs

${specRows(webgpuRows)}

WebGPU chapters: ${chapters("WebGPU")}

WGSL chapters: ${chapters("WGSL")}

Query kinds (\`--domain webgpu\`): ${Object.entries(counts.webgpu ?? {})
  .map(([kind, count]) => `\`${kind}\` ${count}`)
  .join(", ")}.
`;

// references/wasm.md
const wasmRows = specReport.selected.filter((row) =>
  /webassembly/i.test(row.url),
);
const witPackages = [
  ...new Set(of("wasm", "wit").map((entry) => entry.title.split(" ")[1])),
].sort();
const bindgenRelease = releaseSummary["wasm-bindgen/wasm-bindgen"];
const wasmMd = `# WebAssembly, WASI, wasm-bindgen

${banner}

Sources: ${pinLine("wasmSpec")}, ${pinLine("wasi")}, ${pinLine("wasmBindgen")}${bindgenRelease ? ` (latest \`${bindgenRelease.latest}\`, ${bindgenRelease.date.slice(0, 10)})` : ""}, webref ${pinLine("webref")}. Proposal specs keep only the sections the base spec does not have.

${specRows(wasmRows)}

WASI WIT packages: ${witPackages.map((name) => `\`${name}\``).join(" ")}. wasm-bindgen guide: https://wasm-bindgen.github.io/wasm-bindgen/.

Query kinds (\`--domain wasm\`): ${Object.entries(counts.wasm ?? {})
  .map(([kind, count]) => `\`${kind}\` ${count}`)
  .join(", ")}.
`;

// references/specs.md
const otherRows = specReport.selected.filter(
  (row) => !webgpuRows.includes(row) && !wasmRows.includes(row),
);
const specsMd = `# W3C and WHATWG specs

${banner}

Webref crawl ${specReport.crawl} (${pinLine("webref")}). Selected on every sync: every WHATWG standard, plus each W3C/WICG spec that a Modern Web Guidance guide (web-features \`spec\`) or a Bun docs page links to. Sections carry source excerpts where the spec repository's source parsed; definitions are IDL members, CSS properties, elements and algorithms.

${specRows(otherRows)}
`;

const total = rows.length - 1;
const skillMd = `---
name: modern-web
description: Cited, auto-synced reference for Bun (upstream and aphrody-labs fork - docs, Bun.* types, CLI, releases), Modern Web Guidance via bunx (HTML, CSS, client JS), WebGPU/WGSL and wgpu, WebAssembly/WASI/wasm-bindgen, and W3C/WHATWG specs. Use before writing or citing such code.
---

${banner}

# Modern web reference

Search from this directory and quote each hit's \`cite\` (\`repo@commit path:line\` or \`spec (date)\`):

\`\`\`sh
bun scripts/query.ts "<2-6 words>" [--domain bun|mwg|webgpu|wasm|specs] [--kind <kind>] [--limit 8] [--full]
\`\`\`

\`--full\` adds the section text from the pinned source. ${total} entries, pins in \`plugins/aphrody/scripts/modern-web.lock.json\` (${generatedAt.slice(0, 10)}).

| Task | Read | Domain |
| --- | --- | --- |
| Bun API, CLI flag, bunfig, bundler, test runner, release change | [references/bun.md](references/bun.md) | \`bun\` |
| HTML, CSS or client-side JS feature (run \`bunx modern-web-guidance@latest search\` first) | [references/guidance.md](references/guidance.md) | \`mwg\` |
| Chrome extension (Manifest V3, Web Store) | [references/chrome-extensions/index.md](references/chrome-extensions/index.md) | \`mwg\` |
| WebGPU canvas, pipelines, WGSL, wgpu or Naga | [references/webgpu.md](references/webgpu.md) | \`webgpu\` |
| WebAssembly semantics, JS API, WASI, wasm-bindgen | [references/wasm.md](references/wasm.md) | \`wasm\` |
| Fetch, Streams, URL, Encoding, HTML, DOM, WebIDL, CSS | [references/specs.md](references/specs.md) | \`specs\` |
`;

await rm(join(skillDir, "references"), { recursive: true, force: true });
await rm(join(skillDir, "data"), { recursive: true, force: true });
await mkdir(join(skillDir, "references"), { recursive: true });
await mkdir(join(skillDir, "data"), { recursive: true });
await Bun.write(join(skillDir, "SKILL.md"), skillMd);
for (const [file, text] of Object.entries({
  "bun.md": bunMd,
  "guidance.md": guidanceMd,
  "webgpu.md": webgpuMd,
  "wasm.md": wasmMd,
  "specs.md": specsMd,
})) {
  await Bun.write(join(skillDir, "references", file), text);
}
// skills-src/chrome-extensions as an on-demand reference (its SKILL.md becomes index.md so it is not listed twice).
const extensions = join(skillDir, "references", "chrome-extensions");
await cp(join(mwgPin.dir, "skills-src", "chrome-extensions"), extensions, {
  recursive: true,
});
for (const path of await files(extensions, "**/*.md")) {
  const file = join(extensions, path);
  const text = (await Bun.file(file).text())
    .replace(/\bnpx -y\b/g, "bunx")
    .replace(/\b(npx|pnpx)\b/g, "bunx");
  if (path === "SKILL.md") {
    await Bun.write(
      join(extensions, "index.md"),
      `${banner}\n<!-- Source: ${blob(mwgPin, "skills-src/chrome-extensions")} -->\n\n${frontmatter(text).body.trim()}\n`,
    );
    await rm(file);
  } else await Bun.write(file, text);
}
await Bun.write(join(skillDir, "data", "index.jsonl"), `${rows.join("\n")}\n`);
await Bun.write(lockPath, `${JSON.stringify(lock, null, 2)}\n`);

const listing = skillMd.match(/^---\n[\s\S]*?\n---\n/)?.[0] ?? "";
log(
  `wrote ${total} entries; SKILL.md ${skillMd.length} B (frontmatter ${listing.length} B), index ${await Bun.file(join(skillDir, "data", "index.jsonl")).size} B`,
);
