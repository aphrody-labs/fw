#!/usr/bin/env bun
// Archives GitHub repositories that nothing uses (never deletes: archiving is reversible). Every repository is re-checked live
// before it is touched: it must exist and be unarchived, have no push in the last 24 hours, and not be referenced by the manifests
// of the core repositories (`[ecosystem].stack` of ay.toml) nor by the manifests of the submodules they pin. A failed check keeps the repository.
//   bun tools/repo-archive.ts <inventory.tsv> [--apply] [--also owner/name ...]
// The inventory is tab separated with a header whose first column is `repo` (extra columns are ignored). Without --apply nothing is
// changed; the ledger (repo, action, reason) is written to .private/repo-cleanup-<date>.tsv either way.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { privatePath, readAy, repoRoot } from "./local.ts";

export type Verdict = { repo: string; action: "archived" | "would-archive" | "kept" | "failed" | "skipped"; reason: string };

const MANIFESTS = [".gitmodules", "Cargo.toml", "Cargo.lock", "package.json", "bun.lock", "forks.txt", "src.lock.json", "DEPS", "submodules.toml", "forks.toml", "ay.toml", "naming.toml"];
const REF = /(?:aphrody-(?:labs|dev)|rose-griffon)\/([A-Za-z0-9._-]+)|\.\.\/([A-Za-z0-9._-]+?)\.git(?=["'\s)\]]|$)/g;

/** Names (lower case, without .git) that a text mentions as `org/name` or as a relative `../name.git` submodule URL. */
export function referencedNames(text: string): Set<string> {
  const out = new Set<string>();
  for (const m of text.matchAll(REF)) {
    const name = (m[1] ?? m[2] ?? "").replace(/\.git$/, "").toLowerCase();
    if (name) out.add(name);
  }
  return out;
}

export function parseInventory(tsv: string): string[] {
  const rows = tsv.split(/\r?\n/).filter(Boolean);
  const header = rows.shift()?.split("\t") ?? [];
  if (header[0] !== "repo") throw new Error("inventory: first column must be `repo`");
  return [...new Set(rows.map((r) => r.split("\t")[0]!.trim()).filter((r) => /^[\w.-]+\/[\w.-]+$/.test(r)))];
}

const sh = (cmd: string[], cwd?: string) => {
  const p = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  return { code: p.exitCode ?? 1, out: p.stdout.toString(), err: p.stderr.toString().trim() };
};
const gh = (args: string[]) => sh(["gh", "api", ...args]);

/** Manifest text of a core repository at origin/main, from its local checkout (fetched first). */
function coreText(dir: string): string {
  sh(["git", "fetch", "-q", "origin"], dir);
  const files = sh(["git", "ls-tree", "-r", "--name-only", "origin/main"], dir).out.split("\n");
  const pick = files.filter((f) => !/(^|\/)(node_modules|target|third_party)\//.test(f) && (MANIFESTS.includes(f.split("/").pop()!) || /^\.github\/workflows\/[^/]+\.ya?ml$/.test(f)));
  let text = "";
  for (const f of pick) text += `\n${sh(["git", "show", `origin/main:${f}`], dir).out}`;
  return text;
}

/** Manifests at the root of a submodule repository, read through the API (a missing file is fine). */
function remoteText(slug: string): string {
  let text = "";
  for (const f of [".gitmodules", "Cargo.toml", "package.json", "DEPS", "src.lock.json", "forks.txt"]) {
    const r = gh([`repos/${slug}/contents/${f}`, "-H", "Accept: application/vnd.github.raw+json"]);
    if (r.code === 0) text += `\n${r.out}`;
  }
  return text;
}

if (import.meta.main) {
  const argv = Bun.argv.slice(2);
  const apply = argv.includes("--apply");
  const inventory = argv.find((a) => !a.startsWith("--") && argv[argv.indexOf(a) - 1] !== "--also");
  if (!inventory) throw new Error("usage: bun tools/repo-archive.ts <inventory.tsv> [--apply] [--also owner/name ...]");
  const also = argv.flatMap((a, i) => (argv[i - 1] === "--also" ? [a] : []));
  const repos = [...new Set([...parseInventory(readFileSync(inventory, "utf8")), ...also])];

  const ay = readAy(repoRoot) as { ecosystem?: { stack?: string[] }; repos?: Record<string, { slug: string; checkout?: string }> };
  const stack = ay.ecosystem?.stack ?? [];
  const refs = new Set<string>();
  const submodules = new Set<string>();
  for (const key of stack) {
    const checkout = ay.repos?.[key]?.checkout ?? `../${key}`;
    const dir = join(repoRoot, checkout);
    const text = coreText(dir);
    for (const n of referencedNames(text)) refs.add(n);
    for (const m of text.matchAll(/url = (?:\.\.\/)+(?:([\w.-]+)\/)?([\w.-]+?)(?:\.git)?\s*$/gm)) submodules.add(`${m[1] ?? "aphrody-labs"}/${m[2]}`);
    console.error(`core ${key}: ${refs.size} names so far`);
  }
  // Core repositories themselves are never candidates.
  for (const key of stack) refs.add((ay.repos?.[key]?.slug ?? key).split("/").pop()!.toLowerCase());
  for (const slug of submodules) for (const n of referencedNames(remoteText(slug))) refs.add(n);
  console.error(`${submodules.size} submodule repositories read; ${refs.size} referenced names in all`);

  const now = Date.now();
  const verdicts: Verdict[] = [];
  const queue = [...repos];
  const worker = async () => {
    for (let slug = queue.shift(); slug; slug = queue.shift()) {
      const v = (action: Verdict["action"], reason: string): void => void verdicts.push({ repo: slug!, action, reason });
      const name = slug.split("/")[1]!.toLowerCase();
      const live = gh([`repos/${slug}`, "--jq", "[.archived,.pushed_at,.fork,.size]|@tsv"]);
      if (live.code !== 0) { v("failed", `lookup: ${live.err.slice(0, 80)}`); continue; }
      const [archived, pushed] = live.out.trim().split("\t");
      if (archived === "true") { v("skipped", "already archived"); continue; }
      if (now - Date.parse(pushed!) < 24 * 3600_000) { v("kept", `pushed ${pushed} (last 24 h)`); continue; }
      if (refs.has(name)) { v("kept", "referenced by a core repository or a submodule manifest"); continue; }
      if (!apply) { v("would-archive", `unreferenced, last push ${pushed}`); continue; }
      const r = gh(["-X", "PATCH", `repos/${slug}`, "-F", "archived=true", "--jq", ".archived"]);
      if (r.code === 0 && r.out.trim() === "true") v("archived", `unreferenced, last push ${pushed}`);
      else v("failed", `archive: ${r.err.slice(0, 80)}`);
      await Bun.sleep(300);
    }
  };
  await Promise.all([worker(), worker(), worker()]);

  const date = new Date().toISOString().slice(0, 10);
  const ledger = privatePath(`repo-cleanup-${date}.tsv`, repoRoot, true);
  verdicts.sort((a, b) => a.repo.localeCompare(b.repo));
  await Bun.write(ledger, `repo\taction\treason\n${verdicts.map((v) => `${v.repo}\t${v.action}\t${v.reason}`).join("\n")}\n`);
  const count: Record<string, number> = {};
  for (const v of verdicts) count[v.action] = (count[v.action] ?? 0) + 1;
  console.log(JSON.stringify({ ledger, ...count }));
}
