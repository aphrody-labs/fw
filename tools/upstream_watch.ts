#!/usr/bin/env bun
// Live research snapshot of the upstreams flagged `watch = true` in upstreams.toml.
// Writes docs/research/upstream-watch.tsv (stable path, so `git diff` shows what moved upstream).
// Also regenerates plugins/aphrody/skills/upstreams/ (same output as `cargo xtask sync-upstreams`, but needs no Cargo workspace, so CI can run it alone).
// Usage: bun tools/upstream_watch.ts [--check]   (--check exits 1 when the snapshot changed)
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const toml = readFileSync(join(root, "upstreams.toml"), "utf8");

type Spec = { id: string; repo: string; branch: string; fw: string; docs: string; role: string; watch: string };
const specs: Spec[] = toml
  .split("[[upstream]]")
  .slice(1)
  .map((block) => {
    const get = (key: string) => {
      const m = block.match(new RegExp(`^\\s*${key}\\s*=\\s*"?([^"\\n]*)"?\\s*$`, "m"));
      return m ? m[1].trim() : "";
    };
    return { id: get("id"), repo: get("repo"), branch: get("branch") || "main", fw: get("fw"), docs: get("docs"), role: get("role"), watch: get("watch") };
  })
  ;
const watched = specs.filter((s) => s.watch === "true");

async function gh(path: string): Promise<any> {
  const p = Bun.spawn(["gh", "api", path], { stdout: "pipe", stderr: "pipe" });
  const [out, code] = await Promise.all([new Response(p.stdout).text(), p.exited]);
  if (code !== 0) return null;
  try { return JSON.parse(out); } catch { return null; }
}

const rows = await Promise.all(
  watched.map(async (s) => {
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

// ---- catalog (plugins/aphrody/skills/upstreams), mirrors xtask cmd_sync_upstreams
const heads = await Promise.all(
  specs.map(async (s) => {
    const p = Bun.spawn(["git", "ls-remote", `https://github.com/${s.repo}.git`, `refs/heads/${s.branch}`], { stdout: "pipe", stderr: "pipe" });
    const [o, code] = await Promise.all([new Response(p.stdout).text(), p.exited]);
    return code === 0 ? (o.split(/\s+/)[0] || "unknown").slice(0, 10) : "offline";
  }),
);
const refDir = join(root, "plugins", "aphrody", "skills", "upstreams");
const prevTsv = existsSync(join(refDir, "references", "UPSTREAMS.tsv")) ? readFileSync(join(refDir, "references", "UPSTREAMS.tsv"), "utf8") : "";
const prevSha = new Map(prevTsv.split("\n").slice(1).map((l) => l.split("\t")).map((c) => [c[0], c[3]]));
const catTsv = ["id\trepo\tbranch\thead_sha\tfw\tdocs\trole"];
// Same header as xtask cmd_sync_upstreams: generated from upstreams.toml, never hand-written.
let md =
  `---\nname: upstreams\ndescription: >-\n  Git-pinned catalog of the ${specs.length} upstreams fw tracks (${specs.map((s) => s.id).join(", ")}).\n` +
  "  Live HEAD SHAs refresh every 6 hours (sync-upstreams workflow) or with `just sync-upstreams`.\n---\n\n" +
  "# fw upstream catalog\n\nGenerated from `upstreams.toml` by `cargo xtask sync-upstreams` and `bun tools/upstream_watch.ts`; do not edit.\n" +
  "Index: [references/UPSTREAMS.tsv](references/UPSTREAMS.tsv).\n\n| Upstream | HEAD SHA | Target | Docs |\n|---|---|---|---|\n";
specs.forEach((s, i) => {
  const sha = heads[i] === "offline" ? prevSha.get(s.id) ?? "offline" : heads[i]; // keep the last known SHA when the network fails
  catTsv.push([s.id, s.repo, s.branch, sha, s.fw, s.docs, s.role].join("\t"));
  md += `| \`${s.repo}\` | \`${sha}\` | \`${s.fw}\` | [docs](${s.docs}) |\n`;
});
writeFileSync(join(refDir, "references", "UPSTREAMS.tsv"), catTsv.join("\n") + "\n");
writeFileSync(join(refDir, "SKILL.md"), md);
console.log(JSON.stringify({ catalog: specs.length }));
