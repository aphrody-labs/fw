#!/usr/bin/env bun
// hardcoded.ts: measure (and with --check ratchet) the colors, user-facing strings and icon references written as
// literals outside the token, catalog and theme files declared in tools/hardcoded.toml.
//
//   bun tools/hardcoded.ts [--rev REV] [--by unit|area|file|kind] [--top N] [--json]
//   bun tools/hardcoded.ts --check [--rev REV]          fail when a file exceeds its count in the baseline
//   bun tools/hardcoded.ts --update-baseline [--rev REV] rewrite the baseline (only after a reduction)
//
// --rev reads blobs of that commit through `git cat-file`, so a count is reproducible from a commit id alone;
// without it the working tree files listed by `git ls-files` are read. Comments are removed before matching, so
// a color or text in a comment is not counted. The language rules are data in tools/hardcoded.toml.
import { $ } from "bun";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join, resolve } from "node:path";

type Lang = "rust" | "ts" | "css" | "kotlin" | "html";
type Kind = "color" | "string" | "icon";
const KINDS: Kind[] = ["color", "string", "icon"];
type Counts = Record<Kind, number>;
type Hit = { kind: Kind; line: number; text: string; span?: [number, number] };

export const root = resolve(import.meta.dir, "..");
const configPath = join(root, "tools", "hardcoded.toml");
const cfg = Bun.TOML.parse(readFileSync(configPath, "utf8")) as any;

const globs = (list: string[]) => list.map((g) => new Bun.Glob(g));
const excluded = globs(cfg.scan.exclude);
const allowed = globs(cfg.allow.files.map((f: { glob: string }) => f.glob));
const matchesAny = (list: Bun.Glob[], path: string) => list.some((g) => g.match(path));

// ---------------------------------------------------------------- lexing

/** `[from, to, template]`: string literal ranges; `template` marks the literal text of a JS template (not its `${}` code). */
type Range = [number, number, boolean?];
type Lexed = { code: string; strings: Range[] };

/** Replaces comments by spaces (keeping newlines, so offsets and lines stay) and records string literal ranges. */
function lex(text: string, lang: Lang): Lexed {
  const out = text.split("");
  const strings: Range[] = [];
  const n = text.length;
  const blank = (from: number, to: number) => {
    for (let k = from; k < to; k++) if (out[k] !== "\n") out[k] = " ";
  };
  if (lang === "html") {
    for (const m of text.matchAll(/<!--[\s\S]*?-->/g)) blank(m.index!, m.index! + m[0].length);
    return { code: out.join(""), strings };
  }
  const lineComments = lang !== "css";
  const nested = lang === "rust" || lang === "kotlin";

  /** A JS template literal starting at the backtick `i`: literal text segments are ranges, `${}` bodies are code. */
  const template = (i: number): number => {
    let from = i;
    let j = i + 1;
    while (j < n) {
      if (text[j] === "\\") j += 2;
      else if (text[j] === "`") {
        strings.push([from, j + 1, true]);
        return j + 1;
      } else if (text[j] === "$" && text[j + 1] === "{") {
        strings.push([from, j, true]);
        j = code(j + 2, true);
        from = j;
      } else j++;
    }
    strings.push([from, n, true]);
    return n;
  };

  /** Lexes code from `i`; with `stopAtBrace` returns after the `}` that closes the enclosing `${`. */
  const code = (start: number, stopAtBrace: boolean): number => {
    let i = start;
    let depth = 0;
    while (i < n) {
      const c = text[i];
      const next = text[i + 1];
      if (lineComments && c === "/" && next === "/") {
        let j = i;
        while (j < n && text[j] !== "\n") j++;
        blank(i, j);
        i = j;
      } else if (c === "/" && next === "*") {
        let level = 1;
        let j = i + 2;
        while (j < n && level > 0) {
          if (nested && text[j] === "/" && text[j + 1] === "*") { level++; j += 2; }
          else if (text[j] === "*" && text[j + 1] === "/") { level--; j += 2; }
          else j++;
        }
        blank(i, j);
        i = j;
      } else if (lang === "rust" && (c === "r" || (c === "b" && next === "r")) && /^b?r#*"/.test(text.slice(i, i + 12)) && !/\w/.test(text[i - 1] ?? " ")) {
        const m = /^b?r(#*)"/.exec(text.slice(i, i + 12))!;
        const close = `"${m[1]}`;
        const end = text.indexOf(close, i + m[0].length);
        const stop = end < 0 ? n : end + close.length;
        strings.push([i, stop]);
        i = stop;
      } else if (lang === "kotlin" && c === '"' && text.startsWith('"""', i)) {
        const end = text.indexOf('"""', i + 3);
        const stop = end < 0 ? n : end + 3;
        strings.push([i, stop]);
        i = stop;
      } else if (c === "`" && lang === "ts") {
        i = template(i);
      } else if (c === '"' || (c === "'" && lang !== "rust" && lang !== "kotlin")) {
        const multiline = lang === "rust" || lang === "kotlin";
        let j = i + 1;
        while (j < n && text[j] !== c) {
          if (text[j] === "\\") j++;
          else if (!multiline && text[j] === "\n") break;
          j++;
        }
        const stop = Math.min(j + 1, n);
        strings.push([i, stop]);
        i = stop;
      } else if (c === "'" && (lang === "rust" || lang === "kotlin")) {
        // character literal, or a lifetime (no closing quote right after one character)
        if (next === "\\") {
          const end = text.indexOf("'", i + 3);
          i = end < 0 ? i + 1 : end + 1;
        } else if (text[i + 2] === "'") i += 3;
        else i++;
      } else if (stopAtBrace && c === "{") {
        depth++;
        i++;
      } else if (stopAtBrace && c === "}") {
        if (depth === 0) return i + 1;
        depth--;
        i++;
      } else i++;
    }
    return n;
  };

  code(0, false);
  if (lang === "rust") blankTestModules(text, out, strings);
  return { code: out.join(""), strings };
}

/** Blanks `#[cfg(test)] mod name { ... }` bodies: test code is not shipped UI. */
function blankTestModules(text: string, out: string[], strings: Range[]) {
  const start = /#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/g;
  let m: RegExpExecArray | null;
  while ((m = start.exec(text))) {
    if (out[m.index] === " " || inRanges(strings, m.index)) continue;
    let depth = 1;
    let k = m.index + m[0].length;
    while (k < text.length && depth > 0) {
      const at = inRanges(strings, k);
      if (!at && out[k] === "{") depth++;
      else if (!at && out[k] === "}") depth--;
      k++;
    }
    for (let j = m.index; j < k; j++) if (out[j] !== "\n") out[j] = " ";
    start.lastIndex = k;
  }
}

/** Blanks the blocks between the begin and end markers of generated token output (`[scan] generated_blocks`). */
function blankGenerated(code: string): string {
  let out = code;
  for (const [begin, end] of cfg.scan.generated_blocks as [string, string][]) {
    let from = out.indexOf(begin);
    while (from >= 0) {
      const to = out.indexOf(end, from + begin.length);
      if (to < 0) break;
      const stop = to + end.length;
      out = out.slice(0, from) + out.slice(from, stop).replace(/[^\n]/g, " ") + out.slice(stop);
      from = out.indexOf(begin, stop);
    }
  }
  return out;
}

const rangeAt = (ranges: Range[], at: number): Range | undefined => {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const [a, b] = ranges[mid];
    if (at < a) hi = mid - 1;
    else if (at >= b) lo = mid + 1;
    else return ranges[mid];
  }
  return undefined;
};
const inRanges = (ranges: Range[], at: number) => rangeAt(ranges, at) !== undefined;

// ---------------------------------------------------------------- rules

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const alt = (list: string[]) => list.map(escapeRe).join("|");

const colorNamed = new Set<string>(cfg.color.named);
const RE = {
  hex: /(?<![\w&/#-])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})(?![\w-])/g,
  fn: new RegExp(`(?<![\\w-])(?:${alt(cfg.color.functions)})\\(\\s*[-.\\d]`, "gd"),
  namedDecl: new RegExp(`(?<![\\w-])(?:${alt(cfg.color.properties)})["']?\\s*[:=]\\s*["']?([a-z]+)\\b(?!\\s*\\()`, "gi"),
  kotlinColor: /\bColor\(\s*0x[0-9a-fA-F]+/g,
  kotlinNamed: new RegExp(`\\bColor\\.(?:${alt(cfg.color.kotlin_named)})\\b`, "gd"),
  rustCall: new RegExp(`(?<![\\w:])(?:${alt(cfg.string.rust_calls)})\\((?:[^"()]*,\\s*)?"((?:[^"\\\\]|\\\\.)*)"`, "gd"),
  rustMethod: new RegExp(`\\.(?:${alt(cfg.string.rust_methods)})\\(\\s*"((?:[^"\\\\]|\\\\.)*)"\\s*[,)]`, "gd"),
  tsKey: new RegExp(`(?<![\\w.])(?:${alt(cfg.string.ts_keys)})\\s*:\\s*(?:"((?:[^"\\\\\\n]|\\\\.)*)"|'((?:[^'\\\\\\n]|\\\\.)*)'|\`([^\`$\\n]*)\`)`, "gd"),
  attr: new RegExp(`(?<![\\w-])(?:${alt(cfg.string.html_attributes)})=(?:"([^"]*)"|'([^']*)')`, "gd"),
  textNode: /(?<![=-])>\s*([^<>{}\n&`][^<>{}`]*?)\s*</gd,
  kotlinArg: new RegExp(`(?<![\\w.])(?:${alt(cfg.string.kotlin_arguments)})\\s*=\\s*"((?:[^"\\\\]|\\\\.)*)"`, "gd"),
  kotlinCall: new RegExp(`(?<![\\w.])(?:${alt(cfg.string.kotlin_calls)})\\(\\s*"((?:[^"\\\\]|\\\\.)*)"`, "gd"),
  cssContent: /content\s*:\s*"([^"]+)"/g,
  iconKey: new RegExp(`(?<![\\w.])(?:${alt(cfg.icon.keys)})["']?\\s*[:=]\\s*["'\`]([^"'\`$\\n]+)["'\`]`, "gd"),
  iconCall: new RegExp(`(?<![\\w:])(?:${alt(cfg.icon.rust_calls)})\\(\\s*"([^"]+)"`, "gd"),
  iconPath: new RegExp(`"([^"\\n]+(?:${alt(cfg.icon.path_suffixes)})|(?:${alt(cfg.icon.path_prefixes)})[^"\\n]*)"`, "gd"),
  ligature: new RegExp(`<(?:(?:${alt(cfg.icon.ligature_elements)})[^>]*|\\w+[^>]*class=["'][^"']*(?:${alt(cfg.icon.ligature_classes)})[^"']*["'][^>]*)>\\s*([a-z][a-z0-9_]*)\\s*<`, "gd"),
  kotlinIcons: /\bIcons\.(?:Default|Filled|Outlined|Rounded|Sharp|TwoTone|AutoMirrored)(?:\.\w+)+/g,
};

const userFacing = (value: string) => {
  const v = value.trim();
  return v.length >= 2 && /\p{L}/u.test(v) && (/\s/.test(v) || /^\p{Lu}/u.test(v));
};

/** Source offsets of the first capture group that matched (needs the `d` flag). */
function groupSpan(m: RegExpExecArray): [number, number] | undefined {
  const indices = (m as RegExpExecArray & { indices?: ([number, number] | undefined)[] }).indices;
  return indices?.slice(1).find((range) => range !== undefined);
}

export function scan(text: string, lang: Lang, jsx: boolean): Hit[] {
  const lexed = lex(blankGenerated(text), lang);
  const code = lexed.code;
  const strings = lexed.strings;
  const lineStarts = [0];
  for (let k = 0; k < text.length; k++) if (text.charCodeAt(k) === 10) lineStarts.push(k + 1);
  const lineOf = (at: number) => {
    let lo = 0;
    let hi = lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (lineStarts[mid] <= at) lo = mid;
      else hi = mid - 1;
    }
    return lo + 1;
  };
  const inTemplate = (at: number) => rangeAt(strings, at)?.[2] === true;
  const hits: Hit[] = [];
  const seen = new Set<string>();
  const add = (kind: Kind, at: number, text: string, span?: [number, number]) => {
    const key = `${kind}:${at}`;
    if (seen.has(key)) return;
    seen.add(key);
    hits.push({ kind, line: lineOf(at), text: text.slice(0, 80), span });
  };
  const each = (re: RegExp, fn: (m: RegExpExecArray) => void) => {
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(code))) {
      fn(m);
      if (m[0].length === 0) re.lastIndex++;
    }
  };

  // colors
  const hexAnywhere = lang === "css" || lang === "html";
  each(RE.hex, (m) => {
    if (hexAnywhere || inRanges(strings, m.index)) add("color", m.index, m[0]);
  });
  each(RE.fn, (m) => add("color", m.index, m[0]));
  if (lang === "kotlin") {
    each(RE.kotlinColor, (m) => add("color", m.index, m[0]));
    each(RE.kotlinNamed, (m) => add("color", m.index, m[0]));
  } else {
    each(RE.namedDecl, (m) => {
      if (colorNamed.has(m[1].toLowerCase())) add("color", m.index, m[0]);
    });
  }

  // strings
  const text1 = (kind: Kind, m: RegExpExecArray, value: string | undefined) => {
    if (value !== undefined && userFacing(value)) add(kind, m.index, value, groupSpan(m));
  };
  if (lang === "rust") {
    each(RE.rustCall, (m) => text1("string", m, m[1]));
    each(RE.rustMethod, (m) => text1("string", m, m[1]));
  } else if (lang === "ts") {
    each(RE.tsKey, (m) => text1("string", m, m[1] ?? m[2] ?? m[3]));
    each(RE.attr, (m) => {
      if (jsx || inTemplate(m.index)) text1("string", m, m[1] ?? m[2]);
    });
    each(RE.textNode, (m) => {
      if ((jsx || inTemplate(m.index)) && /\p{L}{2}/u.test(m[1]) && !/[=;(){}]|=>/.test(m[1])) add("string", m.index, m[1], groupSpan(m));
    });
  } else if (lang === "html") {
    each(RE.attr, (m) => text1("string", m, m[1] ?? m[2]));
    each(RE.textNode, (m) => {
      if (/\p{L}{2}/u.test(m[1]) && !/[{};]/.test(m[1])) add("string", m.index, m[1], groupSpan(m));
    });
  } else if (lang === "kotlin") {
    each(RE.kotlinArg, (m) => text1("string", m, m[1]));
    each(RE.kotlinCall, (m) => text1("string", m, m[1]));
  } else {
    each(RE.cssContent, (m) => text1("string", m, m[1]));
  }

  // icons
  if (lang === "kotlin") {
    if (cfg.icon.kotlin_icons) each(RE.kotlinIcons, (m) => add("icon", m.index, m[0]));
  } else {
    each(RE.iconKey, (m) => add("icon", m.index, m[1]));
    each(RE.iconCall, (m) => add("icon", m.index, m[1]));
    each(RE.iconPath, (m) => {
      if (lang === "css" || inRanges(strings, m.index) || lang === "html") add("icon", m.index, m[1]);
    });
    if (lang === "ts" || lang === "html") each(RE.ligature, (m) => add("icon", m.index, m[1]));
  }
  return hits;
}

// ---------------------------------------------------------------- file sources

type Entry = { path: string; oid?: string };

async function listFiles(rev: string | undefined): Promise<Entry[]> {
  const langs = cfg.scan.languages as Record<string, Lang>;
  const wanted = (path: string) => extname(path).slice(1) in langs && !matchesAny(excluded, path);
  if (rev) {
    const out = await $`git -C ${root} ls-tree -r -z ${rev}`.text();
    return out
      .split("\0")
      .filter(Boolean)
      .map((line) => {
        const [meta, path] = line.split("\t");
        return { path, oid: meta.split(" ")[2] };
      })
      .filter((e) => wanted(e.path));
  }
  const out = await $`git -C ${root} ls-files -z`.text();
  return out
    .split("\0")
    .filter((p) => p && existsSync(join(root, p)))
    .filter(wanted)
    .map((path) => ({ path }));
}

async function* readAll(entries: Entry[], rev: string | undefined): AsyncGenerator<[Entry, string]> {
  if (!rev) {
    for (const e of entries) yield [e, readFileSync(join(root, e.path), "utf8")];
    return;
  }
  const chunk = 400;
  for (let from = 0; from < entries.length; from += chunk) {
    const part = entries.slice(from, from + chunk);
    const proc = Bun.spawn(["git", "-C", root, "cat-file", "--batch"], { stdin: "pipe", stdout: "pipe" });
    proc.stdin.write(part.map((e) => `${e.oid}\n`).join(""));
    proc.stdin.end();
    const buf = Buffer.from(await new Response(proc.stdout).arrayBuffer());
    let at = 0;
    for (const e of part) {
      const nl = buf.indexOf(10, at);
      const size = Number(buf.subarray(at, nl).toString().split(" ")[2]);
      yield [e, buf.subarray(nl + 1, nl + 1 + size).toString("utf8")];
      at = nl + 1 + size + 1;
    }
    await proc.exited;
  }
}

// ---------------------------------------------------------------- reporting

const unitOf = (path: string) => {
  const segs = path.split("/");
  const prefix = segs[0] === "aphrody" || segs[0] === "ay" ? 1 : 0;
  const head = segs[prefix];
  return ["crates", "packages", "apps", "kotlin", "plugins"].includes(head) && segs.length > prefix + 2
    ? segs.slice(0, prefix + 2).join("/")
    : segs.slice(0, Math.min(prefix + 1, segs.length - 1) || 1).join("/");
};
const areaOf = (path: string) => path.split("/")[0];
const total = (c: Counts) => c.color + c.string + c.icon;
const zero = (): Counts => ({ color: 0, string: 0, icon: 0 });

function loadBaseline(): Map<string, Counts> {
  const file = join(root, cfg.baseline.file);
  const map = new Map<string, Counts>();
  if (!existsSync(file)) return map;
  for (const line of readFileSync(file, "utf8").split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const [path, color, string, icon] = line.split("\t");
    map.set(path, { color: +color, string: +string, icon: +icon });
  }
  return map;
}

async function main() {
  const args = process.argv.slice(2);
  const flag = (name: string) => {
    const i = args.indexOf(name);
    return i >= 0 ? args[i + 1] : undefined;
  };
  const rev = flag("--rev");
  const by = flag("--by") ?? "area";
  const top = Number(flag("--top") ?? 25);
  const langs = cfg.scan.languages as Record<string, Lang>;

  const files = await listFiles(rev);
  const perFile = new Map<string, Counts>();
  const allowedFiles = new Map<string, Counts>();
  const hitsOut: { path: string; hit: Hit }[] = [];
  const wantHits = args.includes("--hits");
  for await (const [entry, text] of readAll(files, rev)) {
    const lang = langs[extname(entry.path).slice(1)];
    const hits = scan(text, lang, /^(tsx|jsx)$/.test(extname(entry.path).slice(1)));
    if (hits.length === 0) continue;
    const counts = zero();
    for (const h of hits) counts[h.kind]++;
    if (matchesAny(allowed, entry.path)) allowedFiles.set(entry.path, counts);
    else {
      perFile.set(entry.path, counts);
      if (wantHits) for (const hit of hits) hitsOut.push({ path: entry.path, hit });
    }
  }

  const sum = zero();
  for (const c of perFile.values()) for (const k of KINDS) sum[k] += c[k];
  const allowedSum = zero();
  for (const c of allowedFiles.values()) for (const k of KINDS) allowedSum[k] += c[k];

  if (args.includes("--update-baseline")) {
    const lines = ["# path\tcolor\tstring\ticon (bun tools/hardcoded.ts --update-baseline)"];
    for (const [path, c] of [...perFile].sort(([a], [b]) => (a < b ? -1 : 1))) lines.push(`${path}\t${c.color}\t${c.string}\t${c.icon}`);
    writeFileSync(join(root, cfg.baseline.file), lines.join("\n") + "\n");
    console.log(`baseline: ${perFile.size} files, ${total(sum)} literals -> ${cfg.baseline.file}`);
    return;
  }

  if (args.includes("--check")) {
    const baseline = loadBaseline();
    const worse: string[] = [];
    for (const [path, c] of perFile) {
      const was = baseline.get(path) ?? zero();
      const grown = KINDS.filter((k) => c[k] > was[k]).map((k) => `${k} ${was[k]} -> ${c[k]}`);
      if (grown.length) worse.push(`${path}: ${grown.join(", ")}`);
    }
    if (worse.length) {
      console.error(`hardcoded: ${worse.length} file(s) above the baseline (${cfg.baseline.file}); move the value to a token, catalog or theme file:`);
      for (const w of worse.slice(0, 50)) console.error(`  ${w}`);
      process.exit(1);
    }
    console.log(`hardcoded: ok, ${total(sum)} literals (color ${sum.color}, string ${sum.string}, icon ${sum.icon}) at or below the baseline`);
    return;
  }

  const group = (key: (p: string) => string) => {
    const map = new Map<string, Counts>();
    for (const [path, c] of perFile) {
      const k = key(path);
      const acc = map.get(k) ?? zero();
      for (const kind of KINDS) acc[kind] += c[kind];
      map.set(k, acc);
    }
    return [...map].sort(([, a], [, b]) => total(b) - total(a));
  };
  const rows = by === "file" ? [...perFile].sort(([, a], [, b]) => total(b) - total(a)) : by === "unit" ? group(unitOf) : group(areaOf);

  if (args.includes("--json")) {
    console.log(JSON.stringify({ rev: rev ?? "worktree", files_scanned: files.length, total: sum, allowed: allowedSum, by: Object.fromEntries(rows.slice(0, top)) }, null, 2));
    return;
  }
  console.log(`rev ${rev ?? "worktree"}: ${files.length} files scanned, ${perFile.size} with literals`);
  console.log(`total ${total(sum)} (color ${sum.color}, string ${sum.string}, icon ${sum.icon}); inside allowlisted token/catalog/theme files: ${total(allowedSum)} in ${allowedFiles.size} files`);
  console.log(`by ${by}:`);
  for (const [key, c] of rows.slice(0, top)) console.log(`${String(total(c)).padStart(7)}  color ${String(c.color).padStart(6)}  string ${String(c.string).padStart(6)}  icon ${String(c.icon).padStart(5)}  ${key}`);
  if (wantHits) for (const { path, hit } of hitsOut.filter((h) => h.path.includes(flag("--only") ?? "")).slice(0, Number(flag("--limit") ?? 200))) console.log(`${path}:${hit.line}: ${hit.kind}: ${hit.text}`);
}

if (import.meta.main) await main();
