#!/usr/bin/env bun
// Archives GitHub repositories that nothing uses (never deletes: archiving is reversible). Every repository is re-checked live
// before it is touched: it must exist and be unarchived, have no push in the last 24 hours, and not be referenced by the manifests
// of the core repositories (`[ecosystem].stack` of ay.toml) nor by the manifests of the submodules they pin. A failed check keeps the repository.
//   bun tools/repo-archive.ts <inventory.tsv> [--apply] [--also owner/name ...]
//   bun tools/repo-archive.ts --restore <ledger.tsv> [--apply]   (unarchive every `archived` row of a ledger)
// The inventory is tab separated with a header whose first column is `repo` (extra columns are ignored). Without --apply nothing is
// changed; the ledger (repo, action, reason) is written to .private/repo-cleanup-<date>-plan.tsv (dry run), -applied.tsv or -restored.tsv, so a
// dry run never overwrites the record of an applied run. A core checkout that cannot be read, or an API error other than 404 on a submodule
// manifest, aborts the run: an unreadable manifest must never count as "nothing references this repository".
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { privatePath, readAy, repoRoot } from "./local.ts";

export interface Verdict { repo: string; action: "archived" | "would-archive" | "kept" | "failed" | "skipped"; reason: string }

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

export function formatLedger(rows: Verdict[]): string {
  return `repo\taction\treason\n${rows.map((v) => `${v.repo}\t${v.action}\t${v.reason}`).join("\n")}\n`;
}

export function parseLedger(tsv: string): Verdict[] {
  const [head, ...rows] = tsv.split(/\r?\n/).filter(Boolean);
  if (head?.split("\t").join() !== "repo,action,reason") throw new Error("ledger: header must be repo, action, reason");
  return rows.map((r) => {
    const [repo, action, ...reason] = r.split("\t");
    return { repo: repo!, action: action as Verdict["action"], reason: reason.join("\t") };
  });
}

const tally = (rows: Verdict[]): Record<string, number> => {
  const count: Record<string, number> = {};
  for (const v of rows) count[v.action] = (count[v.action] ?? 0) + 1;
  return count;
};

const sh = (cmd: string[], cwd?: string) => {
  const p = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  return { code: p.exitCode ?? 1, out: p.stdout.toString(), err: p.stderr.toString().trim() };
};
/** `gh api` with backoff on rate limits (HTTP 403/429 and secondary limits). */
async function ghAsync(args: string[]): Promise<{ code: number; out: string; err: string }> {
  for (let wait = 2; ; wait *= 2) {
    const p = Bun.spawn(["gh", "api", ...args], { stdout: "pipe", stderr: "pipe" });
    const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
    if (code === 0 || wait > 32 || !/rate limit|HTTP 429|abuse|secondary/i.test(err)) return { code, out, err: err.trim() };
    await Bun.sleep(wait * 1000);
  }
}
const gh = (args: string[]) => {
  let r = sh(["gh", "api", ...args]);
  for (let wait = 2; r.code !== 0 && /rate limit|HTTP 429|abuse|secondary/i.test(r.err) && wait <= 32; wait *= 2) {
    Bun.sleepSync(wait * 1000);
    r = sh(["gh", "api", ...args]);
  }
  return r;
};

/** Manifest text of a core repository at origin/main, from its local checkout (fetched first). */
function coreText(dir: string): string {
  if (!existsSync(dir)) throw new Error(`core checkout missing: ${dir}`);
  const fetched = sh(["git", "fetch", "-q", "origin"], dir);
  if (fetched.code !== 0) throw new Error(`git fetch failed in ${dir}: ${fetched.err.slice(0, 120)}`);
  const tree = sh(["git", "ls-tree", "-r", "--name-only", "origin/main"], dir);
  if (tree.code !== 0) throw new Error(`origin/main unreadable in ${dir}: ${tree.err.slice(0, 120)}`);
  const files = tree.out.split("\n");
  const pick = files.filter((f) => !/(^|\/)(node_modules|target|third_party)\//.test(f) && (MANIFESTS.includes(f.split("/").pop()!) || /^\.github\/workflows\/[^/]+\.ya?ml$/.test(f)));
  if (!pick.length) throw new Error(`no manifest found at origin/main in ${dir}`);
  let text = "";
  for (const f of pick) text += `\n${sh(["git", "show", `origin/main:${f}`], dir).out}`;
  return text;
}

/** Manifests at the root of a submodule repository, read through the API in parallel (a missing file is fine, any other error aborts). */
async function remoteText(slug: string): Promise<string> {
  const files = [".gitmodules", "Cargo.toml", "package.json", "DEPS", "src.lock.json", "forks.txt"];
  const parts = await Promise.all(
    files.map(async (f) => {
      const r = await ghAsync([`repos/${slug}/contents/${f}`, "-H", "Accept: application/vnd.github.raw+json"]);
      if (r.code === 0) return `\n${r.out}`;
      if (!/HTTP 404|Not Found/i.test(r.err)) throw new Error(`manifest ${slug}/${f} unreadable: ${r.err.slice(0, 120)}`);
      return "";
    }),
  );
  return parts.join("");
}

if (import.meta.main) {
  const argv = Bun.argv.slice(2);
  const apply = argv.includes("--apply");
  const restore = argv.indexOf("--restore");
  if (restore >= 0) {
    const file = argv[restore + 1];
    if (!file) throw new Error("usage: bun tools/repo-archive.ts --restore <ledger.tsv> [--apply]");
    const rows = parseLedger(readFileSync(file, "utf8")).filter((r) => r.action === "archived");
    const out: Verdict[] = [];
    for (const { repo } of rows) {
      if (!apply) { out.push({ repo, action: "would-archive", reason: "would unarchive" }); continue; }
      const r = gh(["-X", "PATCH", `repos/${repo}`, "-F", "archived=false", "--jq", ".archived"]);
      out.push(r.code === 0 && r.out.trim() === "false" ? { repo, action: "skipped", reason: "unarchived" } : { repo, action: "failed", reason: `unarchive: ${r.err.slice(0, 80)}` });
      await Bun.sleep(300);
    }
    const ledger = privatePath(`repo-cleanup-${new Date().toISOString().slice(0, 10)}-${apply ? "restored" : "restore-plan"}.tsv`, repoRoot, true);
    await Bun.write(ledger, formatLedger(out));
    console.log(JSON.stringify({ ledger, rows: rows.length, ...tally(out) }));
    process.exit(0);
  }
  const inventory = argv.find((a, i) => !a.startsWith("--") && argv[i - 1] !== "--also" && argv[i - 1] !== "--restore");
  if (!inventory) throw new Error("usage: bun tools/repo-archive.ts <inventory.tsv> [--apply] [--also owner/name ...] | --restore <ledger.tsv> [--apply]");
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
  const slugs = [...submodules];
  const readers = Array.from({ length: 6 }, async () => {
    for (let slug = slugs.shift(); slug; slug = slugs.shift()) for (const n of referencedNames(await remoteText(slug))) refs.add(n);
  });
  await Promise.all(readers);
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
  const ledger = privatePath(`repo-cleanup-${date}-${apply ? "applied" : "plan"}.tsv`, repoRoot, true);
  verdicts.sort((a, b) => a.repo.localeCompare(b.repo));
  await Bun.write(ledger, formatLedger(verdicts));
  console.log(JSON.stringify({ ledger, ...tally(verdicts) }));
}
