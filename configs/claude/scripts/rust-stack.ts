#!/usr/bin/env bun
// Catalog of the Rust tooling and crates that matter for fw: docs/research/rust-stack.toml (data) rendered into
// docs/research/rust-stack.md (matrix) with a stale-block gate.
//   bun scripts/rust-stack.ts            rewrite the matrix block of rust-stack.md
//   bun scripts/rust-stack.ts --check    exit 1 when the block is stale or a status contradicts the manifests
//   bun scripts/rust-stack.ts refresh    refill version, downloads and "seen in" from crates.io, GitHub, blessed.rs,
//                                        awesome-rust and lib.rs (network, about 4 minutes at the crates.io 1 request/s policy)
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

export const FW_ROOT = resolve(import.meta.dir, "..", "..", "..");
export const START = "<!-- rust-stack:start -->";
export const END = "<!-- rust-stack:end -->";
export const STATUSES = ["used", "adopt", "reject"] as const;
export type Status = (typeof STATUSES)[number];
export const KINDS = ["tool", "crate", "component", "action"] as const;

export interface Entry {
  name: string;
  kind: (typeof KINDS)[number];
  domain: string;
  fw: Status;
  purpose: string;
  reason: string;
  source: string;
  proof?: string;
  version?: string;
  released?: string;
  downloads?: number;
  recent_downloads?: number;
  seen_in?: string[];
}

export interface Source {
  name: string;
  url: string;
  fetched: string;
  rev: string;
}

export interface Stack {
  sources: Source[];
  entries: Entry[];
}

export function parseStack(toml: string): Stack {
  const raw = Bun.TOML.parse(toml) as { source?: Source[]; entry?: Entry[] };
  const entries = raw.entry ?? [];
  const names = new Set<string>();
  for (const e of entries) {
    const key = `${e.kind}:${e.name}`;
    if (names.has(key)) throw new Error(`duplicate entry ${key}`);
    names.add(key);
    if (!KINDS.includes(e.kind)) throw new Error(`${e.name}: invalid kind ${e.kind}`);
    if (!STATUSES.includes(e.fw)) throw new Error(`${e.name}: invalid status ${e.fw}`);
    for (const f of ["domain", "purpose", "reason", "source"] as const)
      if (!e[f]) throw new Error(`${e.name}: ${f} required`);
    if (!/^https:\/\//.test(e.source)) throw new Error(`${e.name}: source must be an https URL`);
  }
  return { sources: raw.source ?? [], entries };
}

export function summarize(entries: Entry[]): Record<Status, number> & { total: number } {
  const out = { used: 0, adopt: 0, reject: 0, total: entries.length };
  for (const e of entries) out[e.fw]++;
  return out;
}

const esc = (s: string) => s.replaceAll("|", "\\|");
const compact = (n?: number) =>
  n === undefined ? "" : n >= 1e9 ? `${(n / 1e9).toFixed(1)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n);

export function renderStack(stack: Stack): string {
  const { entries, sources } = stack;
  const sum = summarize(entries);
  const lines = [START, ""];
  lines.push(
    `Catalog of ${sum.total} entries (${entries.filter((e) => e.kind !== "crate").length} tools, components and actions, ${entries.filter((e) => e.kind === "crate").length} crates): used ${sum.used}, adopt ${sum.adopt}, reject ${sum.reject}.`,
    "",
    "Sources read live:",
    "",
    ...sources.map((s) => `- ${s.name}: ${s.url} (${s.fetched}${s.rev ? `, ${s.rev}` : ""})`),
    "",
  );
  const domains = [...new Set(entries.map((e) => e.domain))];
  for (const d of domains) {
    const rows = entries.filter((e) => e.domain === d);
    lines.push(
      `### ${d}`,
      "",
      "| name | kind | version | downloads | seen in | fw | purpose and reason |",
      "|---|---|---|---|---|---|---|",
    );
    for (const e of rows)
      lines.push(
        `| [${e.name}](${e.source}) | ${e.kind} | ${e.version ?? ""} | ${compact(e.downloads)} | ${(e.seen_in ?? []).join(", ")} | ${e.fw} | ${esc(e.purpose)}: ${esc(e.reason)} |`,
      );
    lines.push("");
  }
  lines.push(END);
  return lines.join("\n");
}

export function inject(doc: string, block: string): string {
  const a = doc.indexOf(START);
  const b = doc.indexOf(END);
  if (a < 0 || b < 0) throw new Error("rust-stack markers not found");
  return doc.slice(0, a) + block + doc.slice(b + END.length);
}

/** Every external dependency name declared by a Cargo.toml under root (workspace members, excluded crates and workspace.dependencies). */
export function directDependencies(root = FW_ROOT): Set<string> {
  const out = new Set<string>();
  const skip = /(^|\/)(target|node_modules|\.venv|\.wrangler|dist)\//;
  for (const file of new Bun.Glob("**/Cargo.toml").scanSync({ cwd: root, dot: false })) {
    if (skip.test(file.replaceAll("\\", "/"))) continue;
    const t = Bun.TOML.parse(readFileSync(join(root, file), "utf8")) as Record<string, any>;
    const tables: Record<string, any>[] = [t.dependencies, t["dev-dependencies"], t["build-dependencies"], t.workspace?.dependencies];
    for (const target of Object.values<any>(t.target ?? {}))
      tables.push(target.dependencies, target["dev-dependencies"], target["build-dependencies"]);
    for (const table of tables)
      for (const [name, spec] of Object.entries<any>(table ?? {}))
        if (!(spec && typeof spec === "object" && spec.path)) out.add(spec?.package ?? name);
  }
  return out;
}

/**
 * Drift between the catalog and the repo. Errors: a tool, component or action marked `used` without an existing proof path.
 * Warnings: a crate marked `used` that no Cargo.toml declares, or marked `reject` that one does (other lanes add dependencies freely; update the entry).
 */
export function contradictions(entries: Entry[], root = FW_ROOT, direct = directDependencies(root)): { errors: string[]; warnings: string[] } {
  const errors: string[] = [];
  const warnings: string[] = [];
  for (const e of entries) {
    if (e.kind === "crate") {
      if (e.fw === "used" && !direct.has(e.name)) warnings.push(`${e.name}: status used but no Cargo.toml declares it`);
      if (e.fw === "reject" && direct.has(e.name)) warnings.push(`${e.name}: status reject but a Cargo.toml declares it`);
    } else if (e.fw === "used") {
      if (!e.proof) errors.push(`${e.name}: status used needs a proof path`);
      else if (!existsSync(join(root, e.proof))) errors.push(`${e.name}: proof path ${e.proof} missing`);
    }
  }
  return { errors, warnings };
}

const q = (s: string) => JSON.stringify(s);
const FIELDS: (keyof Entry)[] = ["name", "kind", "domain", "fw", "purpose", "reason", "source", "proof", "version", "released", "downloads", "recent_downloads", "seen_in"];

export function stringifyStack(stack: Stack): string {
  const lines = [
    "# Rust tooling and crates that matter for fw. Hand fields: name, kind, domain, fw, purpose, reason, source, proof.",
    "# Live fields (version, released, downloads, recent_downloads, seen_in) and [[source]] come from `bun scripts/rust-stack.ts refresh`.",
    "# fw status: used = declared in a Cargo.toml (crates) or wired into the repo at `proof` (others); adopt = measured or queued with a wire-in point; reject = evaluated, reason given.",
    "",
  ];
  for (const s of stack.sources)
    lines.push("[[source]]", `name = ${q(s.name)}`, `url = ${q(s.url)}`, `fetched = ${q(s.fetched)}`, `rev = ${q(s.rev)}`, "");
  for (const e of stack.entries) {
    lines.push("[[entry]]");
    for (const f of FIELDS) {
      const v = e[f];
      if (v === undefined || v === "" || (Array.isArray(v) && !v.length)) continue;
      lines.push(`${f} = ${Array.isArray(v) ? `[${v.map(q).join(", ")}]` : typeof v === "number" ? v : q(String(v))}`);
    }
    lines.push("");
  }
  return lines.join("\n");
}

const UA = { "User-Agent": "fw-rust-stack (https://github.com/aphrody-labs/fw)" };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const getJson = async (url: string, headers: Record<string, string> = {}) => {
  const r = await fetch(url, { headers: { ...UA, ...headers } });
  return r.ok ? ((await r.json()) as any) : null;
};
const getText = async (url: string) => {
  const r = await fetch(url, { headers: UA });
  return r.ok ? await r.text() : "";
};

const LIBS_CATEGORIES = [
  "development-tools/cargo-plugins",
  "development-tools/testing",
  "development-tools/profiling",
  "development-tools/build-utils",
  "command-line-interface",
  "asynchronous",
  "network-programming",
  "web-programming/http-client",
  "web-programming/http-server",
  "encoding",
  "cryptography",
  "compression",
  "parser-implementations",
  "data-structures",
  "concurrency",
  "memory-management",
  "science/math",
];

export async function refresh(path: string): Promise<void> {
  const stack = parseStack(readFileSync(path, "utf8"));
  const today = new Date().toISOString().slice(0, 10);
  const sources: Source[] = [];

  const blessedUrl = "https://raw.githubusercontent.com/nicoburns/blessed-rs/main/data/crates.json";
  const blessed = await getJson(blessedUrl);
  const blessedNames = new Set<string>();
  for (const g of blessed?.crate_groups ?? [])
    for (const s of g.subgroups ?? [])
      for (const p of s.purposes ?? []) for (const r of p.recommendations ?? []) blessedNames.add(r.name);
  const blessedRev = (await getJson("https://api.github.com/repos/nicoburns/blessed-rs/commits?per_page=1"))?.[0]?.sha?.slice(0, 7) ?? "";
  sources.push({ name: "blessed.rs", url: "https://blessed.rs/crates", fetched: today, rev: blessedRev ? `nicoburns/blessed-rs@${blessedRev}, ${blessedNames.size} crates` : `${blessedNames.size} crates` });

  const awesome = await getText("https://raw.githubusercontent.com/rust-unofficial/awesome-rust/main/README.md");
  const awesomeRev = (await getJson("https://api.github.com/repos/rust-unofficial/awesome-rust/commits?per_page=1"))?.[0]?.sha?.slice(0, 7) ?? "";
  sources.push({ name: "awesome-rust", url: "https://github.com/rust-unofficial/awesome-rust", fetched: today, rev: awesomeRev ? `rust-unofficial/awesome-rust@${awesomeRev}` : "" });

  const libs = new Map<string, Set<string>>();
  for (const slug of LIBS_CATEGORIES) {
    const html = await getText(`https://lib.rs/${slug}`);
    for (const m of html.matchAll(/href="\/crates\/([^"/]+)"/g)) (libs.get(m[1]!) ?? libs.set(m[1]!, new Set()).get(m[1]!)!).add(slug);
    await sleep(300);
  }
  sources.push({ name: "lib.rs categories", url: "https://lib.rs/", fetched: today, rev: `${LIBS_CATEGORIES.length} category pages` });
  sources.push({ name: "crates.io API", url: "https://crates.io/api/v1/crates", fetched: today, rev: "version, downloads, recent_downloads, release date" });
  sources.push({ name: "GitHub API", url: "https://api.github.com/", fetched: today, rev: "releases and tags of non-crates.io tools" });
  sources.push({ name: "rustup components", url: "https://rust-lang.github.io/rustup/concepts/components.html", fetched: today, rev: "rustfmt, clippy, rust-analyzer, miri, llvm-tools" });

  const token = process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {};
  for (const e of stack.entries) {
    const key = e.name.toLowerCase().replaceAll("-", "[-_]");
    const seen: string[] = [];
    if (blessedNames.has(e.name)) seen.push("blessed.rs");
    if (new RegExp(`[\\[/]${key}[\\])/]`, "i").test(awesome)) seen.push("awesome-rust");
    for (const slug of libs.get(e.name) ?? []) seen.push(`lib.rs/${slug}`);
    e.seen_in = seen;
    const github = /^https:\/\/github\.com\/([^/]+\/[^/]+)/.exec(e.source)?.[1];
    if (e.kind === "crate" || (e.kind === "tool" && e.source.includes("crates.io")) || (e.kind === "tool" && !github)) {
      const j = await getJson(`https://crates.io/api/v1/crates/${e.name}`);
      await sleep(1100);
      if (!j) {
        console.error(`crates.io: ${e.name} not found`);
        continue;
      }
      const c = j.crate;
      e.version = c.max_stable_version ?? c.max_version;
      e.released = j.versions?.find((v: any) => v.num === e.version)?.created_at?.slice(0, 10);
      e.downloads = c.downloads;
      e.recent_downloads = c.recent_downloads;
    } else if (github) {
      const rel = (await getJson(`https://api.github.com/repos/${github}/releases/latest`, token)) ?? (await getJson(`https://api.github.com/repos/${github}/tags?per_page=1`, token))?.[0];
      e.version = rel?.tag_name ?? rel?.name;
      e.released = (rel?.published_at ?? "").slice(0, 10) || undefined;
    }
    console.error(`${e.name} ${e.version ?? "-"} ${seen.join(",")}`);
  }
  writeFileSync(path, stringifyStack({ sources, entries: stack.entries }));
}

if (import.meta.main) {
  const dir = join(FW_ROOT, "docs", "research");
  const tomlPath = join(dir, "rust-stack.toml");
  if (Bun.argv.includes("refresh")) {
    await refresh(tomlPath);
    console.log("rust-stack.toml refreshed");
    process.exit(0);
  }
  const stack = parseStack(readFileSync(tomlPath, "utf8"));
  const docPath = join(dir, "rust-stack.md");
  const doc = existsSync(docPath) ? readFileSync(docPath, "utf8") : "";
  const next = inject(doc, renderStack(stack));
  if (Bun.argv.includes("--check")) {
    const { errors, warnings } = contradictions(stack.entries);
    for (const w of warnings) console.error(`rust-stack warning: ${w}`);
    for (const e of errors) console.error(`rust-stack: ${e}`);
    if (next !== doc) console.error("rust-stack.md block is stale: run bun configs/claude/scripts/rust-stack.ts");
    process.exit(errors.length || next !== doc ? 1 : 0);
  }
  writeFileSync(docPath, next);
  const sum = summarize(stack.entries);
  console.log(`rust-stack.md updated: ${sum.total} entries, used ${sum.used}, adopt ${sum.adopt}, reject ${sum.reject}`);
}
