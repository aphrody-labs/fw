#!/usr/bin/env bun
// Live research snapshot of the upstreams flagged `watch = true` in upstreams.toml.
// Writes docs/research/upstream-watch.tsv (stable path, so `git diff` shows what moved upstream).
// Usage: bun tools/upstream_watch.ts [--check]   (--check exits 1 when the snapshot changed)
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const toml = readFileSync(join(root, "upstreams.toml"), "utf8");

type Spec = { id: string; repo: string; branch: string; fw: string };
const specs: Spec[] = toml
  .split("[[upstream]]")
  .slice(1)
  .map((block) => {
    const get = (key: string) => {
      const m = block.match(new RegExp(`^\\s*${key}\\s*=\\s*"?([^"\\n]*)"?\\s*$`, "m"));
      return m ? m[1].trim() : "";
    };
    return { id: get("id"), repo: get("repo"), branch: get("branch") || "main", fw: get("fw"), watch: get("watch") };
  })
  .filter((s: any) => s.watch === "true");

async function gh(path: string): Promise<any> {
  const p = Bun.spawn(["gh", "api", path], { stdout: "pipe", stderr: "pipe" });
  const [out, code] = await Promise.all([new Response(p.stdout).text(), p.exited]);
  if (code !== 0) return null;
  try { return JSON.parse(out); } catch { return null; }
}

const rows = await Promise.all(
  specs.map(async (s) => {
    const [repo, rel, commit] = await Promise.all([
      gh(`repos/${s.repo}`),
      gh(`repos/${s.repo}/releases/latest`),
      gh(`repos/${s.repo}/commits/${s.branch}`),
    ]);
    const day = (d?: string) => (d ? d.slice(0, 10) : "-");
    return [
      s.id,
      repo?.full_name ?? s.repo,
      repo?.stargazers_count ?? "?",
      repo?.license?.spdx_id ?? "-",
      rel?.tag_name ?? "-",
      day(rel?.published_at),
      commit?.sha ? String(commit.sha).slice(0, 10) : "offline",
      day(commit?.commit?.committer?.date),
      repo?.open_issues_count ?? "?",
      s.fw,
    ].join("\t");
  }),
);

const header = "id\trepo\tstars\tlicense\tlatest_release\trelease_date\thead_sha\thead_date\topen_issues_and_prs\tfw";
const out = join(root, "docs", "research", "upstream-watch.tsv");
// Stars and open issues drift every hour; keep them in the file but ignore them when deciding whether anything meaningful changed.
const body = [header, ...rows].join("\n") + "\n";
const stable = (t: string) => t.split("\n").map((l) => l.split("\t").filter((_, i) => ![2, 8].includes(i)).join("\t")).join("\n");
const before = existsSync(out) ? readFileSync(out, "utf8") : "";
const changed = stable(before) !== stable(body);
if (changed || !before) writeFileSync(out, body);
console.log(JSON.stringify({ ok: true, watched: rows.length, changed }));
if (process.argv.includes("--check") && changed) process.exit(1);
