import { homedir } from "node:os";
import { join } from "node:path";
import type { Domain } from "./sources.ts";

export interface Entry {
  domain: Domain;
  kind: string;
  title: string;
  url: string;
  cite: string;
  text: string;
}

export interface Pin {
  repo: string;
  commit: string;
  date: string;
  dir: string;
}

export const pluginRoot = join(import.meta.dir, "..");
export const cacheRoot =
  Bun.env.MODERN_WEB_CACHE ?? join(homedir(), ".cache", "modern-web");
export const excerptLength = 600;

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
export async function checkout(repo: string, sparse: string[]): Promise<Pin> {
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

export async function expandSparse(pin: Pin, sparse: string[]): Promise<void> {
  await git(["sparse-checkout", "set", "--no-cone", ...sparse], pin.dir);
}

export function blob(pin: Pin, path: string, line?: number): string {
  return `https://github.com/${pin.repo}/blob/${pin.commit}/${path}${line ? `#L${line}` : ""}`;
}

export function cite(pin: Pin, path: string, line?: number): string {
  return `${pin.repo}@${pin.commit.slice(0, 12)} ${path}${line ? `:${line}` : ""}`;
}

export async function files(dir: string, pattern: string): Promise<string[]> {
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

export function plain(source: string): string {
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

export function excerpt(source: string): string {
  const text = plain(source);
  if (text.length <= excerptLength) return text;
  const cut = text.lastIndexOf(" ", excerptLength);
  return `${text.slice(0, cut > 0 ? cut : excerptLength)} …`;
}

export interface Section {
  id: string;
  title: string;
  level: number;
  line: number;
  body: string;
}

// Bikeshed markdown headings (`## Title ## {#id}`), HTML headings with ids (Wattsi, .src.html, inline in .bs).
export function bikeshedSections(source: string): Section[] {
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
export function rstSections(source: string): Section[] {
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
export function markdownSections(source: string): Section[] {
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

export function slug(title: string): string {
  return title
    .toLowerCase()
    .replace(/[`*_~]/g, "")
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .trim()
    .replace(/\s+/g, "-");
}

export function frontmatter(source: string): {
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

export async function githubToken(): Promise<string | undefined> {
  const fromEnv = Bun.env.GH_TOKEN ?? Bun.env.GITHUB_TOKEN;
  if (fromEnv) return fromEnv;
  const result = await Bun.$`gh auth token`.quiet().nothrow();
  return result.exitCode === 0 ? result.text().trim() : undefined;
}

export async function githubJson<T>(path: string): Promise<T> {
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

export async function mapLimit<T, R>(
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
