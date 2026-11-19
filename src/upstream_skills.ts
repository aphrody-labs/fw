#!/usr/bin/env bun
/**
 * Auto-generates & updates `plugin/skills/upstreams/SKILL.md` and
 * `plugin/skills/upstreams/references/UPSTREAMS.tsv` directly from `upstreams.toml`
 * and live Git HEAD refs (`gh api` / `git ls-remote`).
 *
 * Zero upstream source trees copied into Git; progressive-disclosure skill + TSV index only.
 */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const TOML_PATH = join(ROOT, "upstreams.toml");
const SKILL_DIR = join(ROOT, "plugin", "skills", "upstreams");
const REF_DIR = join(SKILL_DIR, "references");

interface UpstreamSpec {
  id: string;
  repo: string;
  branch: string;
  docs: string;
  fairwind: string;
  role: string;
}

function parseUpstreamsToml(src: string): UpstreamSpec[] {
  const blocks = src.split(/\[\[upstream\]\]/).slice(1);
  return blocks.map((b) => {
    const get = (k: string) => b.match(new RegExp(`^${k}\\s*=\\s*"([^"]*)"`, "m"))?.[1] ?? "";
    return {
      id: get("id"),
      repo: get("repo"),
      branch: get("branch") || "main",
      docs: get("docs"),
      fairwind: get("fairwind"),
      role: get("role"),
    };
  });
}

function queryHead(repo: string, branch: string): { sha: string; date: string; stars: number } {
  const metaP = Bun.spawnSync(["gh", "api", `repos/${repo}`], { stdout: "pipe", stderr: "pipe" });
  const stars = metaP.exitCode === 0 ? (JSON.parse(metaP.stdout.toString()).stargazers_count ?? 0) : 0;
  const commitP = Bun.spawnSync(["gh", "api", `repos/${repo}/commits/${branch}`], { stdout: "pipe", stderr: "pipe" });
  if (commitP.exitCode === 0) {
    const c = JSON.parse(commitP.stdout.toString());
    return {
      sha: String(c.sha ?? "").slice(0, 10),
      date: String(c.commit?.committer?.date ?? "").slice(0, 10),
      stars,
    };
  }
  const lsP = Bun.spawnSync(["git", "ls-remote", `https://github.com/${repo}.git`, `refs/heads/${branch}`], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const sha = lsP.exitCode === 0 ? lsP.stdout.toString().trim().split(/\s+/)[0]?.slice(0, 10) ?? "unknown" : "unknown";
  return { sha, date: new Date().toISOString().slice(0, 10), stars };
}

const specs = parseUpstreamsToml(readFileSync(TOML_PATH, "utf8"));
const syncedAt = new Date().toISOString().slice(0, 10);

const rows = specs.map((s) => {
  const live = queryHead(s.repo, s.branch);
  return { ...s, ...live };
});

mkdirSync(REF_DIR, { recursive: true });

const tsvLines = [
  "id\trepo\tbranch\thead_sha\tdate\tstars\tfairwind\tdocs\trole",
  ...rows.map((r) => `${r.id}\t${r.repo}\t${r.branch}\t${r.sha}\t${r.date}\t${r.stars}\t${r.fairwind}\t${r.docs}\t${r.role}`),
];
writeFileSync(join(REF_DIR, "UPSTREAMS.tsv"), `${tsvLines.join("\n")}\n`, "utf8");

const skillMd = [
  "---",
  "name: upstreams",
  "description: >-",
  "  Auto-generated Git-pinned upstream reference catalog for Fairwind (llama.cpp, Asterinas,",
  "  Cloudflare OS/Computer/Pingora/Kitesurf/lol-html, rmcp, ZeroClaw, Zed, nix-cachyos-kernel,",
  "  OpenVMM, IronRDP). Keeps zero upstream source bloat in Git; refresh anytime via `just sync-upstreams`.",
  "---",
  "",
  `# Fairwind Auto-Generated Upstream Skill Catalog (Synced ${syncedAt})`,
  "",
  "Refresh live Git HEAD SHAs & doc references via `just sync-upstreams` (`bun src/upstream_skills.ts`).",
  "Machine-readable TSV index: [references/UPSTREAMS.tsv](references/UPSTREAMS.tsv).",
  "",
  "| Upstream | HEAD SHA | Date | Stars | Fairwind Target | Docs |",
  "|---|---|---|---:|---|---|",
  ...rows.map((r) => `| \`${r.repo}\` | \`${r.sha}\` | ${r.date} | ${r.stars} | \`${r.fairwind}\` | [docs](${r.docs}) |`),
  "",
].join("\n");

writeFileSync(join(SKILL_DIR, "SKILL.md"), skillMd, "utf8");
console.log(JSON.stringify({ ok: true, syncedAt, count: rows.length, skillPath: join(SKILL_DIR, "SKILL.md") }));
