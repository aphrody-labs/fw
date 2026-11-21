#!/usr/bin/env bun
// Full-text search (bun:sqlite FTS5) over the JSONL index next to this script; --full prints the section from the pinned source.
// sync.ts copies this file into every generated skill as scripts/query.ts, so a skill works wherever its directory is copied.
// Usage (from a skill directory): bun scripts/query.ts "<query>" [--kind <kind>] [--limit <n>] [--full] [--json]
// Usage (from the plugin root):   bun scripts/query.ts "<query>" [--domain <bun|mwg|webgpu|wasm|specs>] [...]
import { Database } from "bun:sqlite";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

interface Entry {
  domain: string;
  kind: string;
  title: string;
  url: string;
  cite: string;
  text: string;
}

const args = Bun.argv.slice(2);
const option = (name: string) => {
  const index = args.indexOf(name);
  return index >= 0 ? args.splice(index, 2)[1] : undefined;
};
const flag = (name: string) => {
  const index = args.indexOf(name);
  return index >= 0 ? (args.splice(index, 1), true) : false;
};
const kind = option("--kind");
const domain = option("--domain");
const limit = Number(option("--limit") ?? 8);
const full = flag("--full");
const json = flag("--json");
const words = args;
if (!words.length) {
  console.error(
    'usage: bun scripts/query.ts "<query>" [--domain <domain>] [--kind <kind>] [--limit <n>] [--full] [--json]',
  );
  process.exit(2);
}

// A skill has data/ beside scripts/; the plugin root searches every skill's data/.
const root = join(import.meta.dir, "..");
let dataFiles = await Array.fromAsync(
  new Bun.Glob("data/*.jsonl").scan({ cwd: root, absolute: true }),
);
if (!dataFiles.length)
  dataFiles = await Array.fromAsync(
    new Bun.Glob("skills/*/data/*.jsonl").scan({ cwd: root, absolute: true }),
  );
if (!dataFiles.length) {
  console.error(
    `no index under ${root}: run bun scripts/sync.ts in plugins/modern-web`,
  );
  process.exit(1);
}

// The FTS database is rebuilt only when an index file changes.
const stamps = await Promise.all(
  dataFiles
    .sort()
    .map(
      async (file) =>
        `${file}:${(await Bun.file(file).stat()).mtimeMs}:${Bun.file(file).size}`,
    ),
);
const databasePath = join(
  tmpdir(),
  `modern-web-${Bun.hash(stamps.join("\n")).toString(16)}.sqlite`,
);
const fresh = !(await Bun.file(databasePath).exists());
const database = new Database(databasePath);
if (fresh) {
  database.run("PRAGMA journal_mode = OFF");
  database.run(
    `CREATE VIRTUAL TABLE entries USING fts5(title, text, domain UNINDEXED, kind UNINDEXED, url UNINDEXED, cite UNINDEXED, tokenize = "porter unicode61 remove_diacritics 2")`,
  );
  const insert = database.prepare(
    "INSERT INTO entries (title, text, domain, kind, url, cite) VALUES (?, ?, ?, ?, ?, ?)",
  );
  for (const file of dataFiles) {
    const rows = (await Bun.file(file).text())
      .split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line) as Entry);
    database.transaction(() => {
      for (const row of rows)
        insert.run(
          row.title,
          row.text,
          row.domain,
          row.kind,
          row.url,
          row.cite,
        );
    })();
  }
}

const terms = words
  .join(" ")
  .toLowerCase()
  .split(/[^\p{L}\p{N}_$]+/u)
  .filter((word) => word.length > 1);
if (!terms.length) process.exit(0);
const quoted = terms.map((term) => `"${term}"*`);
// Titles weigh 4x the excerpt; all terms first, then any term when nothing matches them all.
const search = (match: string) => {
  const filters = [
    domain ? "AND domain = $domain" : "",
    kind ? "AND kind = $kind" : "",
  ].join(" ");
  const parameters: Record<string, string | number> = {
    $match: match,
    $limit: limit,
  };
  if (domain) parameters.$domain = domain;
  if (kind) parameters.$kind = kind;
  return database
    .query(
      `SELECT title, text, domain, kind, url, cite, bm25(entries, 4.0, 1.0) AS score FROM entries WHERE entries MATCH $match ${filters} ORDER BY score LIMIT $limit`,
    )
    .all(parameters) as (Entry & { score: number })[];
};
let hits = search(quoted.join(" AND "));
if (!hits.length) hits = search(quoted.join(" OR "));

const cacheRoot =
  Bun.env.MODERN_WEB_CACHE ?? join(homedir(), ".cache", "modern-web");
const stripTags = (html: string) =>
  html
    .replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/g, " ")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();

// Section text from the sync cache, else raw.githubusercontent.com at the pinned commit, else the spec page itself.
async function sectionText(hit: Entry): Promise<string> {
  const source = hit.cite.match(
    /([\w.-]+\/[\w.-]+)@([0-9a-f]{7,40}) ([^\s;:]+)(?::(\d+))?/,
  );
  if (source) {
    const [, repo, commit, path, line] = source;
    const cached = Bun.file(join(cacheRoot, repo.replace("/", "__"), path));
    let text: string;
    if (await cached.exists()) text = await cached.text();
    else {
      const pinned = hit.url.match(/\/blob\/([0-9a-f]{40})\//)?.[1] ?? commit;
      const response = await fetch(
        `https://raw.githubusercontent.com/${repo}/${pinned}/${path}`,
      );
      if (!response.ok) return `(source unavailable: HTTP ${response.status})`;
      text = await response.text();
    }
    const lines = text.split("\n");
    const start = Math.max(0, Number(line ?? 1) - 1);
    const heading = /^(#{1,6}\s|<h[1-6]\b|\.\. _)|^[=\-~^]{4,}\s*$/;
    let end = start + 1;
    while (end < lines.length && end - start < 160 && !heading.test(lines[end]))
      end++;
    if (end < lines.length && /^[=\-~^]{4,}\s*$/.test(lines[end])) end--;
    return lines.slice(start, end).join("\n").trim();
  }
  const [page, anchor] = hit.url.split("#");
  if (!anchor) return "";
  const html = await (await fetch(page)).text();
  const at = html.search(
    new RegExp(
      `id=["']?${anchor.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}["'\\s>]`,
    ),
  );
  if (at < 0) return "";
  const rest = html.slice(at);
  const next = rest.slice(1).search(/<h[1-6]\b/);
  return stripTags(
    rest.slice(rest.indexOf(">") + 1, next > 0 ? next + 1 : 20000),
  ).slice(0, 6000);
}

const results = [];
for (const hit of hits)
  results.push({
    ...hit,
    ...(full ? { section: await sectionText(hit) } : {}),
  });
if (json) console.log(JSON.stringify(results, null, 2));
else if (!results.length) console.log(`no hit for "${words.join(" ")}"`);
else
  for (const hit of results) {
    console.log(`## ${hit.title}  [${hit.domain}/${hit.kind}]`);
    console.log(`url:  ${hit.url}`);
    console.log(`cite: ${hit.cite}`);
    if (hit.text) console.log(hit.text);
    if ("section" in hit && hit.section) console.log(`\n${hit.section}`);
    console.log("");
  }
