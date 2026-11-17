#!/usr/bin/env bun
// Bun-native audit of first-party JS/TS: which Node.js APIs, npm packages, scripts, CI steps and Dockerfile lines
// have a Bun replacement, with the proof of each replacement. Rules are derived, not typed:
//   - the n2b registry (apis, cli, globals, modules, packages), downloaded live from crates.io
//     (ay.toml [registries].crates + [audit.bun-native].registry);
//   - the "Instead of X, use Y" table of the installed bun-native skill;
//   - a few structural rules (shebang, lockfile, tsconfig, CI, Dockerfile) whose replacement is proven the same way.
// Every rule is proven against the Bun fork: a docs page and heading, a declaration in bun-types, and a `bun -e` check
// run on the installed Bun (its version is recorded). A rule that fails to prove is reported as unproven.
// Files are read with the parser of Bun (Bun.Transpiler.scan: imports, requires, dynamic imports); call sites are
// matched on source whose strings, comments and template text are masked by a lexer, gated by the file's import graph.
//   bun tools/bun-native.ts rules [--bun <fork checkout>] [--registry <dir>] [--skill <file>] [--out docs/research/bun-native-rules.toml]
//       derive and prove the rules (the only tracked output: rules and proofs, no repository data)
//   bun tools/bun-native.ts audit [--repos a,b] [--work <dir>] [--out <file>]
//       scan the estate with the committed rules; per-repo results go to the private store (.private/bun-native-audit.toml)
//   bun tools/bun-native.ts scan <dir>... [--bun <fork checkout>]     one-off scan of directories, JSON on stdout
//   bun tools/bun-native.ts check <dir>... [--baseline <file>] [--fail-on <severities>] [--write-baseline <file>]
//       gate from the committed dataset (no fork checkout or network): exit 1 on a finding outside the baseline
import { existsSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { privatePath, readAy } from "./local.ts";
import { toml } from "./org-inventory.ts";

const root = resolve(import.meta.dir, "..");
export type Severity = "safe-rewrite" | "behaviour-change" | "no-equivalent";
export type Kind = "import" | "package" | "call" | "cli" | "command" | "global" | "module" | "config" | "ci" | "docker" | "shebang" | "lockfile" | "tsconfig";
export type Proof = { doc?: string; heading?: string; types?: string; check?: string; checkOk?: boolean };
export type Rule = {
  id: string;
  source: string;
  kind: Kind;
  target: string; // what is matched: package name, module, call pattern, command
  replacement: string;
  severity: Severity;
  note?: string;
  match?: string; // call/cli regex source
  flags?: string;
  gate?: string[]; // import specifiers the file must import
  tier?: "bun" | "style"; // style: a Web-standard substitute for a Node API that Bun runs unchanged (Buffer -> Uint8Array...)
  proof: Proof;
  status: "proven" | "unproven";
};
type Json = any;

// ---------------------------------------------------------------- configuration

export type AuditConfig = { registry: string; crates: string; exclude: string[]; mirrors: string[]; legacy: string };
export function loadConfig(dir = root): { cfg: Json; audit: AuditConfig } {
  const cfg = readAy(dir) as Json;
  const a = cfg.audit?.["bun-native"] ?? {};
  return {
    cfg,
    audit: {
      registry: a.registry ?? "n2b-registry",
      crates: cfg.registries?.crates ?? "",
      exclude: a.exclude ?? ["node_modules", "third_party", "vendor", "target", "dist", "generated", ".git"],
      mirrors: a.mirrors ?? [],
      legacy: String(cfg.legacy?.repo ?? "").split("/").pop() ?? "",
    },
  };
}

// ---------------------------------------------------------------- registry (derived rules)

export type Registry = { version: string; files: Record<"apis" | "cli" | "globals" | "modules" | "packages", Json[]> };

/** The registry TOML files: from a directory, or the latest published crate (version read live from crates.io). */
export async function loadRegistry(crate: string, dir?: string): Promise<Registry> {
  const names = ["apis", "cli", "globals", "modules", "packages"] as const;
  const read = async (get: (n: string) => Promise<string>) => {
    const files = {} as Registry["files"];
    for (const n of names) files[n] = (Bun.TOML.parse(await get(n)) as Json)[n];
    return files;
  };
  if (dir) return { version: "local", files: await read((n) => Bun.file(join(dir, `${n}.toml`)).text()) };
  const headers = { "User-Agent": "ay-bun-native-audit" };
  const meta = (await (await fetch(`https://crates.io/api/v1/crates/${crate}`, { headers })).json()) as Json;
  const version: string = meta.crate.max_stable_version ?? meta.crate.max_version;
  const gz = new Uint8Array(await (await fetch(`https://crates.io/api/v1/crates/${crate}/${version}/download`, { headers })).arrayBuffer());
  const entries = await new Bun.Archive(Bun.gunzipSync(gz)).files();
  const find = (n: string) => [...entries].find(([p]) => p.endsWith(`registry/${n}.toml`))?.[1];
  return { version, files: await read(async (n) => (await find(n)?.text()) ?? "") };
}

/** The "Instead of X, use Y" table of the installed bun-native skill (plugin cache, any marketplace). */
export function skillTable(file?: string): Map<string, string> {
  const out = new Map<string, string>();
  const base = join(homedir(), ".claude", "plugins", "cache");
  const files: string[] = [];
  const walk = (d: string, depth: number) => {
    if (depth > 6 || !existsSync(d)) return;
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) walk(p, depth + 1);
      else if (e.name === "SKILL.md" && basename(d) === "bun-native") files.push(p);
    }
  };
  walk(base, 0);
  const newest = file ?? files.sort().at(-1);
  if (!newest) return out;
  let inTable = false;
  for (const line of readFileSync(newest, "utf8").split("\n")) {
    if (line.startsWith("| Instead of")) inTable = true;
    else if (inTable && line.startsWith("|")) {
      const [, from, to] = line.split("|").map((s) => s.trim());
      if (from && to && !from.startsWith("-")) for (const f of from.split(",")) out.set(f.trim(), to);
    } else if (inTable) inTable = false;
  }
  return out;
}

// Replacements the registry leaves open (`<natif>`) or points outside Bun, resolved to a Bun owner. Each is proven like any other.
const overrides: Record<string, { replacement: string; severity: Severity }> = {
  eslint: { replacement: "bun lint", severity: "behaviour-change" },
  "@typescript-eslint/parser": { replacement: "bun lint", severity: "behaviour-change" },
  "@typescript-eslint/eslint-plugin": { replacement: "bun lint", severity: "behaviour-change" },
  "eslint-config-prettier": { replacement: "bun lint", severity: "behaviour-change" },
  prettier: { replacement: "bun fmt", severity: "behaviour-change" },
  concurrently: { replacement: "bun run --parallel", severity: "behaviour-change" },
  chalk: { replacement: "Bun.color", severity: "behaviour-change" },
  kleur: { replacement: "Bun.color", severity: "behaviour-change" },
  colorette: { replacement: "Bun.color", severity: "behaviour-change" },
  picocolors: { replacement: "Bun.color", severity: "behaviour-change" },
  mime: { replacement: "Bun.file", severity: "behaviour-change" },
  "mime-types": { replacement: "Bun.file", severity: "behaviour-change" },
};

const BUILTIN_BIN = new Set(["node", "npx", "npm", "yarn", "pnpm"]);

/** Rust-regex source of the registry to a JS RegExp (inline `(?m)`/`(?i)` flags lifted). */
function compile(pattern: string, extra = "g"): RegExp | undefined {
  let flags = extra;
  const lead = pattern.match(/^\(\?([a-z]+)\)/);
  if (lead) {
    pattern = pattern.slice(lead[0].length);
    for (const f of lead[1]!) if ("mi".includes(f) && !flags.includes(f)) flags += f;
  }
  try {
    return new RegExp(pattern, flags);
  } catch {
    return undefined;
  }
}

const moduleOf = (spec: string) => spec.replace(/^node:/, "");

export function deriveRules(reg: Registry, skill: Map<string, string>): { rules: Rule[]; dropped: Record<string, number> } {
  const rules: Rule[] = [];
  const dropped: Record<string, number> = { info: 0, generic: 0, badRegex: 0, rustAlt: 0 };
  const builtins = new Set((reg.files.modules as Json[]).map((m) => m.module as string));
  const sev = (r: Json, mechanical: boolean): Severity => (mechanical && !r.aggressive ? "safe-rewrite" : "behaviour-change");

  for (const p of reg.files.packages as Json[]) {
    if (p.id !== "imports/bun-native") {
      dropped.rustAlt!++;
      continue;
    }
    const o = overrides[p.package];
    const replacement: string = o?.replacement ?? p.replacement;
    rules.push({
      id: `package/${p.package}`,
      source: "n2b-registry:packages",
      kind: "package",
      target: p.package,
      replacement,
      severity: o?.severity ?? sev(p, !/^<|natif/.test(replacement)),
      note: o ? "replacement resolved from the fork (bun lint, bun fmt, Bun.color, Bun.file)" : undefined,
      proof: {},
      status: "unproven",
    });
  }
  const known = new Set(rules.map((r) => r.target));
  for (const [from, to] of skill) {
    if (known.has(from) || /[\s(]/.test(from) || BUILTIN_BIN.has(from)) continue;
    rules.push({ id: `package/${from}`, source: "skill:bun-native", kind: "package", target: from, replacement: to, severity: "behaviour-change", proof: {}, status: "unproven" });
  }

  for (const a of reg.files.apis as Json[]) {
    if (a.severity === "info") {
      dropped.info!++;
      continue;
    }
    let pattern: string = a.pattern;
    let note: string | undefined;
    if (a.id === "api/child-process-spawn") {
      pattern = String.raw`\b(?:child_process\.)?(?:spawn|spawnSync|execFile|execFileSync|fork)\s*\(`;
      note = "registry pattern matches every call; narrowed to the child_process entry points";
    }
    const re = compile(pattern);
    if (!re) {
      dropped.badRegex!++;
      continue;
    }
    if (/^x\(/.test("x(") && new RegExp(re.source, re.flags).test("x(")) {
      dropped.generic!++;
      continue;
    }
    // The leading identifier of a pattern such as `fs.readFileSync` names a Node module: the file must import it.
    const lead = pattern.match(/^\\b(?:\(\?:)?([a-z_]+)\\?\./)?.[1];
    const gate: string[] = a.import_from ? [a.import_from] : lead && builtins.has(lead) ? [lead, ...(lead === "fs" ? ["fs/promises"] : [])] : [];
    const mechanical = a.replace === "template" || a.replace === "static";
    rules.push({ id: a.id, source: "n2b-registry:apis", kind: "call", target: pattern, replacement: a.replacement ?? apiOf(a.message) ?? "", severity: sev(a, mechanical), note, match: pattern, flags: "g", gate: gate.length ? gate : undefined, proof: {}, status: "unproven" });
  }

  for (const c of reg.files.cli as Json[]) {
    if (!compile(c.pattern)) {
      dropped.badRegex!++;
      continue;
    }
    rules.push({ id: c.id, source: "n2b-registry:cli", kind: "cli", target: c.pattern, replacement: String(c.replace).replace(/\$\{\d\}/g, "").trim(), severity: "behaviour-change", match: c.pattern, flags: "g", proof: {}, status: "unproven" });
  }
  // `npm run x`, `npm test`... swap the runner only; the script content is untouched: safe once `bun` is the project runner.
  for (const r of rules) if (r.kind === "cli" && /^(bun (install|add|remove|update|test|start|run|outdated)\b|bunx)/.test(r.replacement)) r.severity = "safe-rewrite";

  for (const g of reg.files.globals as Json[]) {
    if (g.rewrite !== "template") continue;
    rules.push({ id: g.id, source: "n2b-registry:globals", kind: "global", target: g.symbol, replacement: g.bun, severity: "safe-rewrite", proof: {}, status: "unproven" });
  }

  for (const m of reg.files.modules as Json[]) {
    if (!m.equivalent) continue;
    rules.push({ id: `module/${m.module}`, source: "n2b-registry:modules", kind: "module", target: `node:${m.module}`, replacement: m.equivalent, severity: "behaviour-change", proof: {}, status: "unproven" });
  }
  rules.push({ id: "module/node-prefix", source: "n2b-registry:modules", kind: "module", target: "bare builtin specifier", replacement: "node: prefix", severity: "safe-rewrite", proof: {}, status: "unproven" });

  for (const r of rules) {
    const token = tokensOf(r.replacement)[0];
    if (r.kind === "call" && token && !/^(Bun\.|bun:|bun |bunx|import\.meta|fetch$)/.test(token)) r.tier = "style";
  }
  rules.push(...structural());
  return { rules, dropped };
}

/** First `Bun.x` / `bun:x` / global token of a registry message, when the replacement column is empty. */
const apiOf = (message: string): string | undefined => message.match(/\b(Bun\.[A-Za-z_$][\w$.]*|bun:[a-z]+|crypto\.\w+\(?|queueMicrotask|fetch|atob|Uint8Array|import\.meta\.\w+)/)?.[1]?.replace(/\($/, "");

const rule = (id: string, kind: Kind, target: string, replacement: string, severity: Severity, query?: string): Rule => ({
  id, source: "structural", kind, target, replacement, severity, proof: {}, status: "unproven", note: query,
});
function structural(): Rule[] {
  return [
    rule("shebang/node", "shebang", "#!/usr/bin/env node", "#!/usr/bin/env bun", "behaviour-change", "#!/usr/bin/env bun"),
    rule("command/node", "command", "node <file>", "bun <file>", "behaviour-change", "bun run"),
    rule("lockfile/foreign", "lockfile", "package-lock.json | yarn.lock | pnpm-lock.yaml", "bun.lock", "behaviour-change", "bun.lock"),
    rule("lockfile/missing", "lockfile", "package.json with dependencies and no bun.lock", "bun install", "safe-rewrite", "bun.lock"),
    rule("config/package-manager", "config", "package.json packageManager is not bun", "packageManager: bun@<version>", "safe-rewrite", "packageManager"),
    rule("config/pnpm-workspace", "config", "pnpm-workspace.yaml", "package.json workspaces", "behaviour-change", "workspaces"),
    rule("tsconfig/types-node", "tsconfig", "compilerOptions.types lists node and not bun", "types: [\"bun\"]", "behaviour-change", "@types/bun"),
    rule("tsconfig/no-bun-types", "tsconfig", "tsconfig.json next to package.json without @types/bun", "bun add -d @types/bun", "safe-rewrite", "@types/bun"),
    rule("tsconfig/module-resolution", "tsconfig", "moduleResolution node|node10|classic", "moduleResolution: bundler", "behaviour-change", "moduleResolution"),
    rule("ci/setup-node", "ci", "actions/setup-node", "oven-sh/setup-bun", "behaviour-change", "oven-sh/setup-bun"),
    rule("ci/pnpm-setup", "ci", "pnpm/action-setup", "oven-sh/setup-bun", "behaviour-change", "oven-sh/setup-bun"),
    rule("docker/from-node", "docker", "FROM node", "FROM oven/bun", "behaviour-change", "oven/bun"),
  ];
}

// ---------------------------------------------------------------- proofs

type DocSection = { file: string; heading: string; text: string };
export type Fork = { docs: DocSection[]; types: { file: string; lines: string[] }[]; version?: string };

const walkFiles = (dir: string, ext: RegExp, skip: (p: string) => boolean = () => false, out: string[] = []): string[] => {
  if (!existsSync(dir)) return out;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (skip(p)) continue;
    if (e.isDirectory()) walkFiles(p, ext, skip, out);
    else if (ext.test(e.name)) out.push(p);
  }
  return out;
};

export function loadFork(bun: string): Fork {
  const docs: DocSection[] = [];
  const base = join(bun, "docs");
  for (const f of walkFiles(base, /\.mdx?$/, (p) => /(^|[\\/])(blog|project|node_modules|lint[\\/]rules)([\\/]|$)/.test(relative(base, p)))) {
    const rel = "docs/" + relative(base, f).split(sep).join("/");
    let heading = "";
    let buf: string[] = [];
    let fence = false;
    const flush = () => {
      if (buf.length) docs.push({ file: rel, heading, text: buf.join("\n") });
      buf = [];
    };
    for (const line of readFileSync(f, "utf8").split("\n")) {
      if (line.startsWith("```")) fence = !fence;
      const h = !fence && line.match(/^#{1,4}\s+(.*)$/);
      if (h) {
        flush();
        heading = h[1]!.replace(/\s*\{#.*\}$/, "").trim();
      } else buf.push(line);
    }
    flush();
  }
  const typesDir = join(bun, "packages", "bun-types");
  const types = walkFiles(typesDir, /\.d\.ts$/, (p) => /node_modules|ts7\.1|vendor|scripts/.test(relative(typesDir, p))).map((f) => ({ file: basename(f), lines: readFileSync(f, "utf8").split("\n") }));
  return { docs, types };
}

/** Docs section that best documents one of `needles`: page about it (many mentions, file name), heading naming it, local count. */
export function findDoc(fork: Fork, needles: string | string[], page?: string): { doc: string; heading: string } | undefined {
  const pool = page && fork.docs.some((d) => d.file.includes(page)) ? fork.docs.filter((d) => d.file.includes(page)) : fork.docs;
  let best: { s: number; d: DocSection } | undefined;
  for (const needle of Array.isArray(needles) ? needles : [needles]) {
    const tail = needle.split(/[.: ]/).pop()!.toLowerCase();
    const wordRe = new RegExp(`\\b${tail.replace(/[^\w]/g, "")}\\b`, "i");
    const counts = pool.map((d) => d.text.split(needle).length - 1 + (d.heading.includes(needle) ? 1 : 0));
    const pages = new Map<string, number>();
    pool.forEach((d, i) => pages.set(d.file, (pages.get(d.file) ?? 0) + counts[i]!));
    pool.forEach((d, i) => {
      const n = counts[i]!;
      if (!n) return;
      const s =
        Math.min(n, 6) +
        2 * Math.log2(1 + pages.get(d.file)!) +
        (wordRe.test(d.heading) ? 6 : 0) +
        (basename(d.file, ".mdx").toLowerCase() === tail ? 12 : 0) +
        (d.file.endsWith("/blog.mdx") || d.file.startsWith("docs/blog") ? -50 : 0) +
        (d.heading.includes(needle) ? 4 : 0) -
        d.file.length / 200;
      if (!best || s > best.s) best = { s, d };
    });
  }
  return best && { doc: best.d.file, heading: best.d.heading || "(page)" };
}

/** Declaration of an API path (`Bun.password.hash`, `bun:sqlite`, `fetch`) in bun-types: `file:line name`. */
export function findDecl(fork: Fork, api: string): string | undefined {
  if (/^bun:/.test(api)) {
    for (const t of fork.types) {
      const i = t.lines.findIndex((l) => l.includes(`declare module "${api}"`));
      if (i >= 0) return `${t.file}:${i + 1} module "${api}"`;
    }
    return;
  }
  const segs = api.replace(/^Bun\./, "").split(".");
  const head = segs[0]!;
  const decl = new RegExp(`(?:function|const|let|var|class|namespace|interface|type|enum)\\s+${head.replace(/\$/g, "\\$")}\\b|^\\s*(?:get\\s+)?(?:readonly\\s+)?${head.replace(/\$/g, "\\$")}\\s*[(:<?]`);
  for (const t of fork.types) {
    const i = t.lines.findIndex((l) => decl.test(l));
    if (i < 0) continue;
    const rest = segs.slice(1);
    if (!rest.every((s) => t.lines.slice(i, i + 400).some((l) => new RegExp(`\\b${s}\\b`).test(l)))) continue;
    return `${t.file}:${i + 1} ${api}`;
  }
}

type Probe = { expr: string; args: string[] };
const run = async (cmd: string[], opts: { cwd?: string; env?: Record<string, string> } = {}) => {
  const p = Bun.spawn(cmd, { cwd: opts.cwd, env: { ...process.env, ...opts.env }, stdout: "pipe", stderr: "pipe" });
  const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
  return { out, err, code };
};

/** The runnable check for one API token; `undefined` when the replacement names no concrete Bun feature. */
export function probeFor(token: string, builtins: Set<string>): { cmd: string[]; label: string } | undefined {
  const bunE = (code: string) => ({ cmd: ["bun", "-e", code], label: `bun -e '${code}'` });
  const exit = (cond: string) => `process.exit((${cond})?0:1)`;
  if (/^bun:[a-z]+$/.test(token)) return bunE(`await import("${token}").then(()=>process.exit(0),()=>process.exit(1))`);
  if (/^Bun\.\$$/.test(token)) return bunE(exit(`typeof Bun.$==="function"`));
  if (/^Bun\.[A-Za-z_$][\w$.]*$/.test(token)) {
    const path = token.split(".").slice(1);
    return bunE(exit(`${JSON.stringify(path)}.reduce((o,k)=>o!=null&&k in Object(o)?o[k]:undefined,Bun)!==undefined`));
  }
  if (token === "import.meta.dir" || token === "import.meta.path") return bunE(exit(`typeof ${token}==="string"`));
  if (token === "fetch") return bunE(exit(`typeof fetch==="function"`));
  const mod = token.match(/^([a-z_]+)\.(\w+)$/);
  if (mod && !/^(crypto|process)$/.test(mod[1]!) && builtins.has(mod[1]!)) return bunE(`const m=await import("node:${mod[1]}");${exit(`typeof m.${mod[2]}==="function"`)}`);
  if (/^[A-Z]\w+$/.test(token) || /^(crypto|process)\.\w+$/.test(token)) return bunE(exit(`${JSON.stringify(token.split("."))}.reduce((o,k)=>o?.[k],globalThis)!==undefined`));
  if (token === "queueMicrotask" || token === "atob") return bunE(exit(`typeof ${token}==="function"`));
  if (token === "bun run") return { cmd: ["bun", "run", "--help"], label: "bun run --help" };
  const cli = token.match(/^bun (add|install|remove|update|test|start|run|outdated|init|link|publish|lint|fmt|migrate|run|build|pm pack|x)\b/);
  if (cli) return { cmd: ["bun", ...cli[1]!.split(" "), "--help"], label: `bun ${cli[1]} --help` };
  if (token === "bunx") return { cmd: ["bunx", "--help"], label: "bunx --help" };
  if (token === "bun --watch") return { cmd: ["bun", "--help"], label: "bun --help (lists --watch)" };
  if (token === "bun run --parallel") return { cmd: ["bun", "run", "--help"], label: "bun run --help (lists --parallel)" };
  if (token === "@types/bun") return bunE(`const r=await fetch("https://registry.npmjs.org/@types/bun/latest");${exit("r.ok")}`);
  if (token === "bun build") return { cmd: ["bun", "build", "--help"], label: "bun build --help" };
}

const TOKEN = new RegExp(
  [
    String.raw`Bun\.\$`,
    String.raw`Bun\.[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*`,
    String.raw`bun:[a-z]+`,
    String.raw`import\.meta\.\w+`,
    String.raw`bun (?:pm pack|install|add|remove|update|test|start|outdated|init|link|publish|lint|fmt|migrate|run|x|build)\b`,
    String.raw`bun --watch`,
    String.raw`bun run --parallel`,
    String.raw`bunx`,
    String.raw`bun \(Glob\)`,
    String.raw`<global fetch>`,
    String.raw`<auto>`,
    String.raw`Web Worker`,
    String.raw`queueMicrotask|atob|fetch`,
    String.raw`crypto\.\w+`,
    String.raw`[A-Z][A-Za-z0-9]+`,
    String.raw`[a-z_]+\.[a-z]\w+`,
  ].join("|"),
);
const canonical: Record<string, string> = { "<global fetch>": "fetch", "bun (Glob)": "Bun.Glob", "Web Worker": "Worker" };

/** Concrete API tokens named by a replacement column: the first one of each ` / ` alternative. */
export function tokensOf(replacement: string): string[] {
  if (replacement === "bun start") return ["bun run"];
  if (replacement === "node: prefix" || replacement === "oven-sh/setup-bun") return [replacement];
  const out = new Set<string>();
  for (const part of replacement.split(" / ")) {
    const m = part.match(TOKEN)?.[0];
    if (m) out.add(canonical[m] ?? m);
  }
  return [...out];
}

/** Where a token is documented: the API itself, its class/leaf name, or the node: module page for a Node API. */
function docNeedlesOf(token: string, builtins: Set<string>): string[] {
  const mod = token.match(/^([a-z_]+)\.\w+$/)?.[1];
  if (mod && builtins.has(mod) && !/^(crypto|process)$/.test(mod)) return [`node:${mod}`];
  if (token === "<auto>") return [".env"];
  if (/^Bun\./.test(token)) return [token, token.split(".")[1]!];
  return [token];
}

const pageOf = (token: string, builtins: Set<string>) => {
  const mod = token.match(/^([a-z_]+)\.\w+$/)?.[1];
  return mod && builtins.has(mod) && !/^(crypto|process)$/.test(mod) ? "nodejs-compat" : undefined;
};

/** Proves every rule in place: docs section, bun-types declaration, runtime check. Returns the Bun version used. */
export async function proveRules(rules: Rule[], fork: Fork, builtins: Set<string>): Promise<{ bunVersion: string; revision: string }> {
  const bunVersion = (await run(["bun", "--version"])).out.trim();
  const revision = (await run(["bun", "-e", "console.log(Bun.revision.slice(0,8))"])).out.trim();
  const cache = new Map<string, Proof & { ok: boolean }>();
  const staticQuery: Record<string, { token: string; docs: string[]; page?: string }> = {
    "shebang/node": { token: "bun run", docs: ["#!/usr/bin/env bun"] },
    "command/node": { token: "bun run", docs: ["bun run"] },
    "lockfile/foreign": { token: "bun install", docs: ["bun.lock"], page: "pm/lockfile" },
    "lockfile/missing": { token: "bun install", docs: ["bun.lock"], page: "pm/lockfile" },
    "config/package-manager": { token: "bun install", docs: ["packageManager", "bun install"] },
    "config/pnpm-workspace": { token: "bun install", docs: ["workspaces"] },
    "tsconfig/types-node": { token: "@types/bun", docs: ["@types/bun"], page: "typescript" },
    "tsconfig/no-bun-types": { token: "@types/bun", docs: ["@types/bun"], page: "typescript" },
    "tsconfig/module-resolution": { token: "@types/bun", docs: ["moduleResolution"], page: "typescript" },
    "ci/setup-node": { token: "oven-sh/setup-bun", docs: ["oven-sh/setup-bun"], page: "cicd" },
    "ci/pnpm-setup": { token: "oven-sh/setup-bun", docs: ["oven-sh/setup-bun"], page: "cicd" },
    "docker/from-node": { token: "bun run", docs: ["oven/bun"], page: "docker" },
    "module/node-prefix": { token: "node: prefix", docs: ["node:fs"], page: "nodejs-compat" },
  };

  const proveToken = async (token: string, docNeedles: string[], page?: string): Promise<Proof & { ok: boolean }> => {
    const key = `${token}\u0000${docNeedles.join("\u0000")}\u0000${page ?? ""}`;
    const hit = cache.get(key);
    if (hit) return hit;
    const proof: Proof & { ok: boolean } = { ok: false };
    const d = findDoc(fork, docNeedles, page);
    if (d) {
      proof.doc = d.doc;
      proof.heading = d.heading;
    }
    proof.types = findDecl(fork, token.replace(/^bun \(Glob\)$/, "Bun.Glob").replace(/^<global fetch>$/, "fetch"));
    if (token === "<auto>") {
      const dir = join(tmpdir(), `ay-bn-${crypto.randomUUID()}`);
      await Bun.write(join(dir, ".env"), "AY_PROBE=1\n");
      const r = await run(["bun", "-e", "process.exit(process.env.AY_PROBE===\"1\"?0:1)"], { cwd: dir });
      proof.check = `bun -e 'process.exit(process.env.AY_PROBE==="1"?0:1)' (cwd with a .env file)`;
      proof.ok = r.code === 0;
      proof.types ??= "n/a (behaviour, documented)";
      (await import("node:fs")).rmSync(dir, { recursive: true, force: true });
    } else if (token === "node: prefix" || token === "oven-sh/setup-bun") {
      proof.check = token === "node: prefix" ? `bun -e 'await import("node:fs")'` : "n/a (CI action, documented)";
      proof.ok = token === "node: prefix" ? (await run(["bun", "-e", `await import("node:fs")`])).code === 0 : !!proof.doc;
      proof.types ??= "n/a";
    } else {
      const probe = probeFor(token, builtins);
      if (probe) {
        const r = await run(probe.cmd);
        proof.check = probe.label;
        proof.ok = r.code === 0 && (token !== "bun --watch" || /--watch/.test(r.out)) && (token !== "bun run --parallel" || /--parallel/.test(r.out));
      }
    }
    cache.set(key, proof);
    return proof;
  };

  const queue = [...rules];
  const worker = async () => {
    for (let r = queue.pop(); r; r = queue.pop()) {
      const st = staticQuery[r.id];
      let tokens: string[];
      let docNeedles: string[];
      if (st) {
        tokens = [st.token];
        docNeedles = st.docs;
      } else if (r.kind === "package" || r.kind === "module" || r.kind === "global" || r.kind === "call") {
        tokens = tokensOf(r.replacement);
        docNeedles = tokens.length ? tokens : [r.replacement];
      } else if (r.kind === "cli") {
        tokens = tokensOf(r.replacement);
        docNeedles = tokens;
      } else {
        tokens = [];
        docNeedles = [];
      }
      if (!tokens.length) {
        r.status = "unproven";
        r.severity = "no-equivalent";
        continue;
      }
      const proofs = await Promise.all(tokens.map((t) => proveToken(t, st ? docNeedles : docNeedlesOf(t, builtins), st?.page ?? pageOf(t, builtins))));
      const p = proofs[0]!;
      r.proof = { doc: p.doc, heading: p.heading, types: proofs.map((x) => x.types).find(Boolean), check: proofs.map((x) => x.check).filter(Boolean).join(" && ") || undefined, checkOk: proofs.every((x) => x.ok) };
      r.status = r.proof.checkOk && (r.proof.doc || r.proof.types) ? "proven" : "unproven";
      if (r.status === "unproven") r.severity = "no-equivalent";
    }
  };
  await Promise.all(Array.from({ length: 8 }, worker));
  return { bunVersion, revision };
}

// ---------------------------------------------------------------- lexer: which offsets are code

/** 1 for offsets that are code (including `${}` of templates), 0 for strings, template text, comments and regex literals. */
export function codeMask(src: string): Uint8Array {
  const mask = new Uint8Array(src.length);
  const stack: number[] = []; // brace depth at which a template expression resumes the template
  let depth = 0;
  let i = 0;
  let prev = ""; // last significant code char, to tell a regex from a division
  const n = src.length;
  const template = () => {
    while (i < n) {
      const c = src[i]!;
      if (c === "\\") i += 2;
      else if (c === "`") {
        i++;
        return;
      } else if (c === "$" && src[i + 1] === "{") {
        mask[i] = mask[i + 1] = 1;
        i += 2;
        stack.push(depth++);
        return;
      } else i++;
    }
  };
  while (i < n) {
    const c = src[i]!;
    const d = src[i + 1];
    if (c === "/" && d === "/") {
      while (i < n && src[i] !== "\n") i++;
    } else if (c === "/" && d === "*") {
      const e = src.indexOf("*/", i + 2);
      i = e < 0 ? n : e + 2;
    } else if (c === '"' || c === "'") {
      i++;
      while (i < n && src[i] !== c && src[i] !== "\n") i += src[i] === "\\" ? 2 : 1;
      i++;
      prev = "a";
    } else if (c === "`") {
      i++;
      template();
    } else if (c === "/" && !/[\w$)\]]/.test(prev)) {
      i++;
      let cls = false;
      while (i < n && src[i] !== "\n" && (cls || src[i] !== "/")) {
        if (src[i] === "\\") i++;
        else if (src[i] === "[") cls = true;
        else if (src[i] === "]") cls = false;
        i++;
      }
      i++;
      prev = "a";
    } else {
      if (c === "{") depth++;
      if (c === "}") {
        depth--;
        if (stack.length && stack[stack.length - 1] === depth) {
          stack.pop();
          i++;
          template();
          continue;
        }
      }
      mask[i] = 1;
      if (!/\s/.test(c)) prev = c;
      i++;
    }
  }
  return mask;
}

// ---------------------------------------------------------------- scanning

export type Hit = { rule: string; file: string; line: number };
export type RepoResult = {
  files: { scanned: number; js: number; parseErrors: number; configs: number };
  hits: Hit[];
  errorFiles: string[];
};

const JS = /\.(?:[cm]?[jt]s|[jt]sx)$/;
const loaderOf = (f: string) => (f.endsWith("x") ? (f.includes("t") ? "tsx" : "jsx") : /\.[cm]?ts$/.test(f) ? "ts" : "js");
const transpilers: Record<string, Bun.Transpiler> = {};
const transpiler = (l: string) => (transpilers[l] ??= new Bun.Transpiler({ loader: l as "js" }));

const specPkg = (s: string) => (s.startsWith("@") ? s.split("/").slice(0, 2).join("/") : s.split("/")[0]!);
const lineAt = (starts: number[], off: number) => {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid]! <= off) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
};

/** Command words of a shell line, in command position (after `&&`, `||`, `;`, `|`, `(`; env assignments skipped). */
export function commandWords(line: string): string[] {
  const out: string[] = [];
  for (const seg of line.split(/&&|\|\||[;|(]/)) {
    const words = seg.trim().split(/\s+/);
    let k = 0;
    while (k < words.length && (/^\w+=/.test(words[k]!) || ["sudo", "exec", "time", "env"].includes(words[k]!))) k++;
    if (words[k]) out.push(words[k]!.replace(/^["']|["']$/g, ""));
  }
  return out;
}

export class Scanner {
  byId = new Map<string, Rule>();
  pkg = new Map<string, Rule>();
  calls: { rule: Rule; re: RegExp }[] = [];
  cli: { rule: Rule; re: RegExp }[] = [];
  globals: Rule[] = [];
  modules = new Map<string, Rule>();
  builtins: Set<string>;
  constructor(rules: Rule[], builtins: Set<string>, onlyProven = true) {
    this.builtins = builtins;
    for (const r of rules) {
      if (onlyProven && r.status !== "proven") continue;
      this.byId.set(r.id, r);
      if (r.kind === "package") {
        this.pkg.set(r.target, r);
        if (/^(bun( |:test)|fs\.(rm|mkdir)$)/.test(r.replacement)) this.bins.add(r.target);
      }
      else if (r.kind === "call" && r.match) this.calls.push({ rule: r, re: compile(r.match, "g")! });
      else if (r.kind === "cli" && r.match) this.cli.push({ rule: r, re: compile(r.match, "g")! });
      else if (r.kind === "global") this.globals.push(r);
      else if (r.kind === "module" && r.id !== "module/node-prefix") this.modules.set(r.target, r);
    }
  }
  has = (id: string) => this.byId.has(id);
  /** Packages that are command-line tools (their Bun owner is a bun command or a node:fs call): `which` or `glob` as a shell word is the Unix tool. */
  bins = new Set<string>();

  /** Rules matched by one shell line (a package.json script, a `run:` line, a Dockerfile RUN/CMD, a .sh line). */
  shellLine(line: string, emit: (id: string) => void) {
    for (const { rule, re } of this.cli) {
      re.lastIndex = 0;
      if (re.test(line)) emit(rule.id);
    }
    for (const w of commandWords(line)) {
      if (w === "node" && this.has("command/node")) emit("command/node");
      else if (this.bins.has(w)) emit(`package/${w}`);
    }
  }

  jsFile(file: string, code: string, deps: Set<string>, out: Hit[], onError: () => void) {
    const hit = (rule: string, line: number) => out.push({ rule, file, line });
    const starts = [0];
    for (let i = code.indexOf("\n"); i >= 0; i = code.indexOf("\n", i + 1)) starts.push(i + 1);
    if (/^#!\s*(?:\/usr\/bin\/env\s+(?:-S\s+)?node|\/usr\/bin\/node)\b/.test(code) && this.has("shebang/node")) hit("shebang/node", 1);
    let imports: { path: string; kind: string }[];
    let exports: string[];
    try {
      ({ imports, exports } = transpiler(loaderOf(file)).scan(code));
    } catch {
      onError();
      return;
    }
    const specs = new Set<string>();
    const lineOfSpec = (s: string) => {
      const m = new RegExp(`["']${s.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&")}["']`).exec(code);
      return m ? lineAt(starts, m.index) : 1;
    };
    for (const imp of imports) {
      if (!["import-statement", "require-call", "dynamic-import", "require-resolve"].includes(imp.kind)) continue;
      const spec = imp.path;
      if (spec.startsWith(".") || spec.startsWith("/") || spec.startsWith("bun:")) continue;
      specs.add(moduleOf(spec));
      const mod = moduleOf(spec);
      if (spec.startsWith("node:")) {
        const m = this.modules.get(spec);
        if (m) hit(m.id, lineOfSpec(spec));
        continue;
      }
      if (this.builtins.has(spec)) {
        if (!deps.has(spec) && this.has("module/node-prefix")) hit("module/node-prefix", lineOfSpec(spec));
        continue;
      }
      const r = this.pkg.get(spec) ?? this.pkg.get(specPkg(spec));
      if (r) hit(r.id, lineOfSpec(spec));
      void mod;
    }
    const mask = codeMask(code);
    for (const { rule, re } of this.calls) {
      if (rule.gate && !rule.gate.some((g) => specs.has(g))) continue;
      re.lastIndex = 0;
      for (let m = re.exec(code); m; m = re.exec(code)) {
        if (m[0] === "") re.lastIndex++;
        else if (mask[m.index]) {
          if (code[m.index - 1] === ".") continue; // member of another object (`re.exec(`, `Bun.TOML.parse(`)
          hit(rule.id, lineAt(starts, m.index));
        }
      }
    }
    const esm = exports.length > 0 || imports.some((i) => i.kind === "import-statement");
    for (const g of this.globals) {
      if (!esm) continue;
      for (let i = code.indexOf(g.target); i >= 0; i = code.indexOf(g.target, i + 1)) {
        const before = code[i - 1] ?? "";
        const after = code[i + g.target.length] ?? "";
        if (mask[i] && !/[\w$.]/.test(before) && !/[\w$]/.test(after)) hit(g.id, lineAt(starts, i));
      }
    }
  }
}

const textOf = async (path: string) => {
  try {
    const f = Bun.file(path);
    return f.size > 2_000_000 ? undefined : await f.text();
  } catch {
    return undefined;
  }
};

/** Tracked files of a checkout (`git ls-files`), else a directory walk. */
async function listFiles(dir: string, exclude: Set<string>): Promise<string[]> {
  let files: string[] = [];
  if (existsSync(join(dir, ".git"))) {
    const r = await run(["git", "-C", dir, "ls-files", "-z"]);
    if (r.code === 0) files = r.out.split("\0").filter(Boolean);
  }
  if (!files.length) {
    const walk = (d: string) => {
      for (const e of readdirSync(d, { withFileTypes: true })) {
        if (exclude.has(e.name)) continue;
        const p = join(d, e.name);
        if (e.isDirectory()) walk(p);
        else files.push(relative(dir, p).split(sep).join("/"));
      }
    };
    walk(dir);
  }
  return files.filter((f) => !f.split("/").some((s) => exclude.has(s)) && !/\.min\.[cm]?js$|\.d\.[cm]?ts$/.test(f));
}

const lockForeign = new Set(["package-lock.json", "npm-shrinkwrap.json", "yarn.lock", "pnpm-lock.yaml"]);
const isDocker = (f: string) => /(^|\/)(Dockerfile(\.[\w.-]+)?|[\w.-]+\.Dockerfile)$/.test(f);

export async function scanRepo(dir: string, sc: Scanner, exclude: Set<string>): Promise<RepoResult> {
  const files = await listFiles(dir, exclude);
  const res: RepoResult = { files: { scanned: 0, js: 0, parseErrors: 0, configs: 0 }, hits: [], errorFiles: [] };
  const fileSet = new Set(files);
  // dependency names of every package.json: a builtin name such as `events` that is declared there is an npm package
  const deps = new Set<string>();
  const pkgs = new Map<string, Json>();
  for (const f of files.filter((f) => f.endsWith("package.json") && basename(f) === "package.json")) {
    try {
      const j = JSON.parse((await textOf(join(dir, f))) ?? "{}");
      pkgs.set(f, j);
      for (const k of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) for (const d of Object.keys(j[k] ?? {})) deps.add(d);
    } catch {
      res.files.parseErrors++;
    }
  }
  const emit = (rule: string, file: string, line = 1) => {
    if (sc.has(rule)) res.hits.push({ rule, file, line });
  };
  const hasLock = (d: string) => {
    for (let cur = d; ; cur = dirname(cur)) {
      if (fileSet.has(cur === "." ? "bun.lock" : `${cur}/bun.lock`) || fileSet.has(cur === "." ? "bun.lockb" : `${cur}/bun.lockb`)) return true;
      if (cur === ".") return false;
    }
  };

  const tsPackages: string[] = [];
  const queue = [...files];
  const work = async () => {
    for (let f = queue.pop(); f; f = queue.pop()) {
      const name = basename(f);
      const d = dirname(f);
      if (lockForeign.has(name)) {
        emit("lockfile/foreign", f);
        continue;
      }
      if (name === "pnpm-workspace.yaml") {
        emit("config/pnpm-workspace", f);
        continue;
      }
      if (JS.test(f)) {
        const code = await textOf(join(dir, f));
        if (code === undefined || /@generated|DO NOT EDIT|AUTOGENERATED/i.test(code.slice(0, 400))) continue;
        res.files.js++;
        res.files.scanned++;
        sc.jsFile(f, code, deps, res.hits, () => {
          res.files.parseErrors++;
          res.errorFiles.push(f);
        });
      } else if (name === "package.json") {
        const j = pkgs.get(f);
        if (!j) continue;
        res.files.configs++;
        res.files.scanned++;
        for (const k of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) {
          for (const dep of Object.keys(j[k] ?? {})) {
            const r = sc.pkg.get(dep);
            if (r) emit(r.id, f);
          }
        }
        for (const s of Object.values<string>(j.scripts ?? {})) if (typeof s === "string") sc.shellLine(s, (id) => emit(id, f));
        if (j.packageManager && !String(j.packageManager).startsWith("bun@")) emit("config/package-manager", f);
        const all = { ...j.dependencies, ...j.devDependencies };
        if (Object.keys(all).length && !hasLock(d)) emit("lockfile/missing", f);
        if (existsSync(join(dir, d, "tsconfig.json"))) tsPackages.push(f);
      } else if (/^tsconfig(\.[\w-]+)?\.json$/.test(name)) {
        const t = await textOf(join(dir, f));
        res.files.configs++;
        res.files.scanned++;
        try {
          const co = (Bun.JSONC.parse(t ?? "{}") as Json)?.compilerOptions ?? {};
          const types: string[] = co.types ?? [];
          if (types.includes("node") && !types.includes("bun") && !types.includes("bun-types")) emit("tsconfig/types-node", f);
          if (/^(node|node10|classic)$/i.test(String(co.moduleResolution ?? ""))) emit("tsconfig/module-resolution", f);
        } catch {
          res.files.parseErrors++;
        }
      } else if (/(^|\/)\.github\/(workflows\/[^/]+|actions\/[^/]+\/action)\.ya?ml$/.test(f)) {
        const t = await textOf(join(dir, f));
        res.files.configs++;
        res.files.scanned++;
        try {
          const y = Bun.YAML.parse(t ?? "") as Json;
          const steps: Json[] = [...Object.values<Json>(y?.jobs ?? {}).flatMap((j) => j?.steps ?? []), ...(y?.runs?.steps ?? [])];
          for (const s of steps) {
            const uses = String(s?.uses ?? "");
            if (/^actions\/setup-node@/.test(uses)) emit("ci/setup-node", f);
            if (/^pnpm\/action-setup@/.test(uses)) emit("ci/pnpm-setup", f);
            if (typeof s?.run === "string") for (const line of s.run.split("\n")) sc.shellLine(line, (id) => emit(id, f));
          }
        } catch {
          res.files.parseErrors++;
        }
      } else if (isDocker(f)) {
        const t = (await textOf(join(dir, f))) ?? "";
        res.files.configs++;
        res.files.scanned++;
        // a continued instruction is one logical line reported at the line where it starts
        const logical: { text: string; at: number }[] = [];
        let pending: { text: string; at: number } | undefined;
        t.split("\n").forEach((raw, i) => {
          const piece = raw.replace(/\\\r?$/, "");
          pending = pending ? { at: pending.at, text: `${pending.text} ${piece}` } : { text: piece, at: i + 1 };
          if (!/\\\r?$/.test(raw)) {
            logical.push(pending);
            pending = undefined;
          }
        });
        if (pending) logical.push(pending);
        logical.forEach(({ text: line, at }) => {
          const m = line.match(/^\s*(FROM|RUN|CMD|ENTRYPOINT)\s+(.*)$/i);
          if (!m) return;
          const verb = m[1]!.toUpperCase();
          if (verb === "FROM") {
            if (/^(--\S+\s+)*(\S+\/)?node(:|@|\s|$)/.test(m[2]!)) emit("docker/from-node", f, at);
            return;
          }
          let body = m[2]!;
          if (verb !== "RUN" && body.startsWith("[")) {
            try {
              body = (JSON.parse(body) as string[]).join(" ");
            } catch {}
          }
          sc.shellLine(body, (id) => emit(id, f, at));
        });
      } else if (/\.(?:sh|bash)$/.test(name)) {
        const t = (await textOf(join(dir, f))) ?? "";
        res.files.scanned++;
        t.split("\n").forEach((line, i) => {
          if (/^\s*#/.test(line) && !(i === 0 && line.startsWith("#!"))) return;
          if (i === 0 && /^#!\s*(\/usr\/bin\/env\s+node|\/usr\/bin\/node)\b/.test(line)) emit("shebang/node", f, 1);
          else if (!/^\s*#/.test(line)) sc.shellLine(line, (id) => emit(id, f, i + 1));
        });
      }
    }
  };
  await Promise.all(Array.from({ length: 16 }, work));
  // one finding per repository: no package of it declares the Bun types
  const typed = [...pkgs.values()].some((p) => "@types/bun" in { ...p.devDependencies, ...p.dependencies } || "bun-types" in { ...p.devDependencies, ...p.dependencies });
  if (tsPackages.length && !typed) emit("tsconfig/no-bun-types", tsPackages.sort((a, b) => a.length - b.length || a.localeCompare(b))[0]!);
  return res;
}

// ---------------------------------------------------------------- fixes

/** The file declares its own `name` (variable, function, destructuring, arrow parameter): its uses are not the module global. */
const declares = (name: string) => new RegExp(String.raw`\b(?:const|let|var|function|class)\s+${name}\b|\b(?:const|let|var)\s*[{\[][^}\]]*\b${name}\b|\(\s*(?:[\w$]+\s*,\s*)*${name}\s*(?:,[^)]*)?\)\s*=>`);

/**
 * Rewrites of the safe class that are deterministic on one file: a bare builtin specifier gets its `node:` prefix; `__dirname` and
 * `__filename` of an ES module become `import.meta.dir` and `import.meta.path` (the same values on Bun). Returns the new text, or
 * the same text when nothing applies. Runner swaps (`npm ci` to `bun install`) are not here: they need the Bun setup of the CI or image.
 */
export function fixSource(code: string, rules: Set<string>, deps: Set<string>, builtins: Set<string>): string {
  let out = code;
  if (rules.has("module/node-prefix")) {
    for (const mod of builtins) {
      if (deps.has(mod)) continue;
      const re = new RegExp(String.raw`(\bfrom\s*|\brequire\s*\(\s*|\bimport\s*\(\s*|\bimport\s+)(["'])${mod.replace(/[/]/g, "\\/")}\2`, "g");
      const mask = codeMask(out);
      out = out.replace(re, (m, pre: string, quote: string, offset: number) => (mask[offset] ? `${pre}${quote}node:${mod}${quote}` : m));
    }
  }
  for (const [rule, name, to] of [
    ["globals/__dirname", "__dirname", "import.meta.dir"],
    ["globals/__filename", "__filename", "import.meta.path"],
  ] as const) {
    if (!rules.has(rule) || declares(name).test(out) || !/\b(?:import|export)\s/.test(out)) continue;
    const mask = codeMask(out);
    let next = "";
    let at = 0;
    for (let i = out.indexOf(name); i >= 0; i = out.indexOf(name, i + 1)) {
      const before = out[i - 1] ?? "";
      const after = out[i + name.length] ?? "";
      if (mask[i] && !/[\w$.]/.test(before) && !/[\w$]/.test(after)) {
        next += out.slice(at, i) + to;
        at = i + name.length;
      }
    }
    out = next + out.slice(at);
  }
  return out;
}

/** Applies fixSource to every file the scan flagged with a fixable rule; returns the changed files. */
export async function fixRepo(dir: string, sc: Scanner, exclude: Set<string>, builtins: Set<string>): Promise<string[]> {
  const fixable = new Set(["module/node-prefix", "globals/__dirname", "globals/__filename"].filter((r) => sc.byId.get(r)?.severity === "safe-rewrite"));
  const res = await scanRepo(dir, sc, exclude);
  const byFile = new Map<string, Set<string>>();
  for (const h of res.hits) if (fixable.has(h.rule)) (byFile.get(h.file) ?? byFile.set(h.file, new Set()).get(h.file)!).add(h.rule);
  const deps = new Set<string>();
  for (const f of await listFiles(dir, exclude)) {
    if (basename(f) !== "package.json") continue;
    try {
      const j = JSON.parse((await textOf(join(dir, f))) ?? "{}");
      for (const k of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) for (const d of Object.keys(j[k] ?? {})) deps.add(d);
    } catch {}
  }
  const changed: string[] = [];
  for (const [file, rules] of byFile) {
    const code = await Bun.file(join(dir, file)).text();
    const next = fixSource(code, rules, deps, builtins);
    if (next !== code) {
      await Bun.write(join(dir, file), next);
      changed.push(file);
    }
  }
  return changed.sort();
}

// ---------------------------------------------------------------- reports

export function summarize(res: RepoResult, rules: Map<string, Rule>, top = 10, unsafeCap = 200) {
  const sev: Record<string, number> = { "safe-rewrite": 0, "behaviour-change": 0, "no-equivalent": 0 };
  let style = 0;
  const byRule: Record<string, number> = {};
  const byFile = new Map<string, number>();
  for (const h of res.hits) {
    const r = rules.get(h.rule);
    if (!r) continue;
    if (r.tier === "style") {
      style++;
      continue;
    }
    sev[r.severity]!++;
    byRule[h.rule] = (byRule[h.rule] ?? 0) + 1;
    byFile.set(h.file, (byFile.get(h.file) ?? 0) + 1);
  }
  const sorted = Object.fromEntries(Object.entries(byRule).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])));
  const unsafe = res.hits
    .filter((h) => rules.get(h.rule)?.severity !== "safe-rewrite" && rules.get(h.rule)?.tier !== "style")
    .sort((a, b) => a.rule.localeCompare(b.rule) || a.file.localeCompare(b.file) || a.line - b.line);
  return {
    files: res.files,
    parse_error_files: res.errorFiles.sort().slice(0, 20),
    total: res.hits.length - style,
    style_hits: style,
    severity: sev,
    rules: sorted,
    top_files: [...byFile].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, top).map(([path, hits]) => ({ path, hits })),
    unsafe_total: unsafe.length,
    unsafe: unsafe.slice(0, unsafeCap).map((h) => ({ rule: h.rule, severity: rules.get(h.rule)!.severity, file: h.file, line: h.line })),
  };
}

// ---------------------------------------------------------------- repository selection

/** Repositories in scope: ay.toml [repos] (minus runtime forks) and the non-archived originals of the org inventory. */
export function selectRepos(cfg: Json, audit: AuditConfig, inventory?: Json): { key: string; slug: string; checkout?: string }[] {
  const out = new Map<string, { key: string; slug: string; checkout?: string }>();
  for (const [key, r] of Object.entries<Json>(cfg.repos ?? {})) if (r.kind !== "runtime") out.set(r.slug, { key, slug: r.slug, checkout: r.checkout ?? (key === cfg.product?.repo ? "." : undefined) });
  for (const [org, o] of Object.entries<Json>(inventory?.github ?? {})) {
    for (const r of o.originals ?? []) {
      const slug = `${org}/${r.name}`;
      if (r.archived || r.name === audit.legacy || audit.mirrors.includes(r.name) || out.has(slug)) continue;
      out.set(slug, { key: r.name, slug });
    }
  }
  return [...out.values()];
}

// ---------------------------------------------------------------- CLI

const arg = (flag: string) => {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? process.argv[i + 1] : undefined;
};

async function prepare(bunDir: string | undefined, registryDir: string | undefined, skillFile = arg("--skill")) {
  const { cfg, audit } = loadConfig();
  const reg = await loadRegistry(`${audit.crates}${audit.registry}`, registryDir);
  const checkout = bunDir ?? resolve(root, cfg.repos?.bun?.checkout ?? "../bun");
  if (!existsSync(join(checkout, "docs"))) throw new Error(`Bun fork docs not found at ${checkout}: pass --bun <checkout>`);
  const fork = loadFork(checkout);
  const builtins = new Set((reg.files.modules as Json[]).map((m) => m.module as string));
  const { rules, dropped } = deriveRules(reg, skillTable(skillFile));
  const { bunVersion, revision } = await proveRules(rules, fork, builtins);
  return { cfg, audit, reg, rules, dropped, builtins, bunVersion, revision };
}

/** Paths the audit reads; a partial clone materialises only these, so a multi-gigabyte repository costs its first-party sources. */
const sparse = ["*.js", "*.jsx", "*.mjs", "*.cjs", "*.ts", "*.tsx", "*.mts", "*.cts", "package.json", "tsconfig*.json", "Dockerfile*", "*.Dockerfile", "*.sh", "*.bash", "/.github/workflows/*.yml", "/.github/workflows/*.yaml", ".github/actions/*/action.yml", "pnpm-workspace.yaml"];

/** A local checkout, else a shallow partial clone of the slug limited to the audited paths (blobs fetched in one batch). */
async function resolveCheckout(r: { key: string; slug: string; checkout?: string }, work: string): Promise<{ dir: string; temp: boolean } | undefined> {
  if (r.checkout && existsSync(resolve(root, r.checkout))) return { dir: resolve(root, r.checkout), temp: false };
  const dir = join(work, r.key);
  if (!existsSync(join(dir, ".git"))) {
    const c = await run(["gh", "repo", "clone", r.slug, dir, "--", "--depth", "1", "--filter=blob:none", "--no-checkout", "--quiet"]);
    if (c.code !== 0) return undefined;
    await run(["git", "-C", dir, "sparse-checkout", "set", "--no-cone", ...sparse]);
    if ((await run(["git", "-C", dir, "checkout", "--quiet"])).code !== 0) return undefined;
  }
  return { dir, temp: true };
}

export const rulesPath = join(root, "docs/research/bun-native-rules.toml");

/** The proven rules as a TOML document: deterministic, no repository, host or account data. */
export function rulesDocument(p: { rules: Rule[]; reg: Registry; dropped: Record<string, number>; builtins: Set<string>; bunVersion: string; revision: string; audit: AuditConfig; skillRules: number }): string {
  const proven = p.rules.filter((r) => r.status === "proven").length;
  const doc = {
    meta: {
      command: "bun tools/bun-native.ts rules",
      bun_version: p.bunVersion,
      bun_revision: p.revision,
      registry_crate: `${p.audit.crates}${p.audit.registry}`,
      registry_version: p.reg.version,
      builtins: [...p.builtins],
      rules: p.rules.length,
      proven,
      unproven: p.rules.length - proven,
      skill_rules: p.skillRules,
      dropped: p.dropped,
      scan: "imports by Bun.Transpiler.scan; call sites matched on lexer-masked source gated by the import graph; configs parsed (JSONC, YAML, Dockerfile, shell lines)",
      severity_meaning: "safe-rewrite = swap with identical behaviour per the registry; behaviour-change = Bun owner exists, edge cases differ; no-equivalent = no proven Bun owner",
    },
    rule: p.rules.map((r) => ({
      id: r.id, source: r.source, kind: r.kind, target: r.target, replacement: r.replacement, severity: r.severity, status: r.status,
      doc: r.proof.doc ?? "", heading: r.proof.heading ?? "", types: r.proof.types ?? "", check: r.proof.check ?? "", check_ok: r.proof.checkOk ?? false,
      ...(r.gate ? { gate: r.gate } : {}),
      ...(r.tier ? { tier: r.tier } : {}),
      ...(r.note ? { note: r.note } : {}),
    })),
  };
  return `# Generated by tools/bun-native.ts rules; rerun it, do not edit. Rules and their proofs only: no repository, host or account data.\n${toml(doc)}`;
}

/** Rules and builtins read back from a rules document (no fork checkout, registry download or network). */
export function loadRules(path = rulesPath): { rules: Rule[]; builtins: Set<string>; meta: Json } {
  const t = Bun.TOML.parse(readFileSync(path, "utf8")) as Json;
  return {
    meta: t.meta,
    builtins: new Set<string>(t.meta.builtins),
    rules: (t.rule as Json[]).map((r) => ({
      id: r.id, source: r.source, kind: r.kind, target: r.target, replacement: r.replacement, severity: r.severity, status: r.status,
      match: r.kind === "call" || r.kind === "cli" ? r.target : undefined, flags: "g", gate: r.gate, tier: r.tier,
      proof: { doc: r.doc, heading: r.heading, types: r.types, check: r.check, checkOk: r.check_ok },
    })),
  };
}

/** Non-archived source repositories of the organizations of ay.toml [orgs], read live from GitHub. */
export async function liveOriginals(orgs: string[]): Promise<Json> {
  const github: Record<string, Json> = {};
  for (const org of orgs) {
    const r = await run(["gh", "repo", "list", org, "--limit", "1000", "--source", "--no-archived", "--json", "name,isArchived"]);
    if (r.code === 0) github[org] = { originals: (JSON.parse(r.out) as { name: string; isArchived: boolean }[]).map((x) => ({ name: x.name, archived: x.isArchived })) };
  }
  return { github };
}

if (import.meta.main) {
  const [cmd, ...rest] = process.argv.slice(2);
  const dirs = rest.filter((a, i) => !a.startsWith("--") && !rest[i - 1]?.startsWith("--"));
  if (cmd === "rules") {
    const p = await prepare(arg("--bun"), arg("--registry"));
    const skillRules = p.rules.filter((r) => r.source === "skill:bun-native").length;
    await Bun.write(resolve(root, arg("--out") ?? rulesPath), rulesDocument({ ...p, skillRules }));
    console.error(`${p.rules.length} rules, ${p.rules.filter((r) => r.status === "proven").length} proven on Bun ${p.bunVersion}`);
  } else if (cmd === "audit") {
    const { cfg, audit } = loadConfig();
    const loaded = loadRules();
    const invPath = privatePath("org-inventory.toml");
    const inventory = existsSync(invPath) ? (Bun.TOML.parse(readFileSync(invPath, "utf8")) as Json) : await liveOriginals(Object.values<string>(cfg.orgs ?? {}));
    let repos = selectRepos(cfg, audit, inventory);
    const only = arg("--repos")?.split(",");
    if (only) repos = repos.filter((r) => only.includes(r.key));
    const work = arg("--work") ?? join(tmpdir(), "ay-bun-native");
    const exclude = new Set(audit.exclude);
    const sc = new Scanner(loaded.rules, loaded.builtins);
    const byId = new Map(loaded.rules.map((r) => [r.id, r]));
    const repo: Record<string, Json> = {};
    for (const r of repos) {
      const co = await resolveCheckout(r, work);
      if (!co) {
        repo[r.key] = { error: "checkout unavailable" };
        continue;
      }
      repo[r.key] = summarize(await scanRepo(co.dir, sc, exclude), byId);
      if (co.temp) rmSync(co.dir, { recursive: true, force: true });
      console.error(`${r.key}: ${repo[r.key]!.total} hits in ${repo[r.key]!.files.scanned} files`);
    }
    const totals = { safe: 0, behaviour: 0, none: 0, style: 0 };
    for (const v of Object.values(repo)) if (v.severity) {
      totals.safe += v.severity["safe-rewrite"];
      totals.behaviour += v.severity["behaviour-change"];
      totals.none += v.severity["no-equivalent"];
      totals.style += v.style_hits;
    }
    const report = { meta: { command: "bun tools/bun-native.ts audit", bun_version: loaded.meta.bun_version, registry_version: loaded.meta.registry_version, rules: loaded.meta.rules, proven: loaded.meta.proven }, totals, repo };
    const text = `# Generated by tools/bun-native.ts audit; rerun it, do not edit. Private: stays out of git.\n${toml(report)}`;
    const out = arg("--out");
    await Bun.write(out ? resolve(out) : privatePath("bun-native-audit.toml", root, true), text);
  } else if (cmd === "fix") {
    const { rules, builtins } = loadRules(resolve(root, arg("--rules") ?? "docs/research/bun-native-rules.toml"));
    const sc = new Scanner(rules, builtins);
    const exclude = new Set(loadConfig().audit.exclude);
    for (const d of dirs) for (const f of await fixRepo(resolve(d), sc, exclude, builtins)) console.log(f);
  } else if (cmd === "scan" || cmd === "check") {
    // The committed rules carry every proven rule: no fork checkout, registry download or network is needed to scan.
    const rulesFile = resolve(root, arg("--rules") ?? "docs/research/bun-native-rules.toml");
    const useRules = !arg("--bun") && !arg("--registry") && existsSync(rulesFile);
    const cfgs = loadConfig();
    let rules: Rule[];
    let builtins: Set<string>;
    if (useRules) ({ rules, builtins } = loadRules(rulesFile));
    else {
      const p = await prepare(arg("--bun"), arg("--registry"));
      rules = p.rules;
      builtins = p.builtins;
    }
    const sc = new Scanner(rules, builtins);
    const byId = new Map(rules.map((r) => [r.id, r]));
    const exclude = new Set(cfgs.audit.exclude);
    const failOn = new Set((arg("--fail-on") ?? "safe-rewrite,behaviour-change,no-equivalent").split(","));
    const baselinePath = arg("--baseline");
    const baseline = new Set<string>(baselinePath && existsSync(baselinePath) ? readFileSync(baselinePath, "utf8").split("\n").filter(Boolean) : []);
    const seen = new Set<string>();
    let fail = 0;
    for (const d of dirs.length ? dirs : ["."]) {
      const res = await scanRepo(resolve(d), sc, exclude);
      if (cmd === "scan") console.log(JSON.stringify({ dir: basename(resolve(d)), ...summarize(res, byId) }, null, 1));
      else
        for (const h of res.hits) {
          const key = `${h.rule} ${h.file}`;
          const r = byId.get(h.rule)!;
          if (!failOn.has(r.severity) || (r.tier === "style" && !arg("--style"))) continue;
          seen.add(key);
          if (baseline.has(key)) continue;
          fail++;
          console.error(`${h.file}:${h.line}: ${h.rule}: use ${r.replacement} (${r.severity})`);
        }
    }
    const writeBaseline = arg("--write-baseline");
    if (writeBaseline) await Bun.write(writeBaseline, [...seen].sort().join("\n") + "\n");
    if (cmd === "check") process.exit(fail && !writeBaseline ? 1 : 0);
  } else {
    console.error("usage: bun tools/bun-native.ts audit|scan|check ...");
    process.exit(2);
  }
}

export { run as _run };
