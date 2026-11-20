#!/usr/bin/env bun
// Skill catalog audit: frontmatter lint, name collisions and near-duplicate descriptions across every skill root
// (repo packages, repo skills/, installed marketplace plugins). The listing cost is what each skill adds to every session.
//   bun scripts/skills.ts [--json] [--roots dir,dir]
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { REPO_ROOT, toSlash } from "./lib.ts";

export interface Skill {
  name: string;
  description: string;
  path: string;
  source: string;
  hidden: boolean;
}

export interface Issue {
  kind:
    | "missing-name"
    | "missing-description"
    | "name-mismatch"
    | "duplicate-name"
    | "near-duplicate";
  skill: string;
  other?: string;
  detail: string;
}

export function parseFrontmatter(text: string): Record<string, unknown> {
  const m = text.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!m) return {};
  try {
    const parsed = Bun.YAML.parse(m[1]!);
    return parsed && typeof parsed === "object"
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

function skillFiles(dir: string, depth = 5): string[] {
  if (!existsSync(dir) || depth < 0) return [];
  const out: string[] = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      if (
        e.name === "node_modules" ||
        e.name === ".git" ||
        e.name === "references" ||
        e.name === "upstream"
      )
        continue;
      out.push(...skillFiles(p, depth - 1));
    } else if (e.name === "SKILL.md") out.push(p);
  }
  return out;
}

export function loadSkills(root: string, source: string): Skill[] {
  return skillFiles(root).map((file) => {
    const fm = parseFrontmatter(readFileSync(file, "utf8"));
    const dirName = toSlash(file).split("/").slice(-2)[0] ?? "";
    return {
      name: typeof fm.name === "string" ? fm.name : "",
      description: typeof fm.description === "string" ? fm.description : "",
      path: toSlash(file),
      source,
      hidden: fm.hidden === true,
      ...(dirName ? { dirName } : {}),
    } as Skill;
  });
}

const STOP = new Set([
  "the",
  "a",
  "an",
  "and",
  "or",
  "of",
  "to",
  "for",
  "in",
  "on",
  "with",
  "use",
  "when",
  "is",
  "are",
  "any",
  "by",
  "from",
  "that",
  "this",
  "it",
  "as",
]);

export function tokens(text: string): Set<string> {
  return new Set(
    text
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .filter((w) => w.length > 2 && !STOP.has(w)),
  );
}

export function jaccard(a: Set<string>, b: Set<string>): number {
  if (!a.size || !b.size) return 0;
  let inter = 0;
  for (const t of a) if (b.has(t)) inter++;
  return inter / (a.size + b.size - inter);
}

export function audit(skills: Skill[], threshold = 0.6): Issue[] {
  const issues: Issue[] = [];
  const byName = new Map<string, Skill[]>();
  for (const s of skills) {
    if (!s.name)
      issues.push({
        kind: "missing-name",
        skill: s.path,
        detail: "frontmatter has no name",
      });
    if (!s.description)
      issues.push({
        kind: "missing-description",
        skill: s.path,
        detail: "frontmatter has no description",
      });
    const dir = s.path.split("/").slice(-2)[0];
    if (s.name && dir && s.name !== dir)
      issues.push({
        kind: "name-mismatch",
        skill: s.path,
        detail: `name ${s.name} differs from directory ${dir}`,
      });
    if (s.name) byName.set(s.name, [...(byName.get(s.name) ?? []), s]);
  }
  for (const [name, group] of byName) {
    if (group.length > 1)
      issues.push({
        kind: "duplicate-name",
        skill: group[0]!.path,
        other: group[1]!.path,
        detail: `${name} defined ${group.length} times (${group.map((g) => g.source).join(", ")})`,
      });
  }
  const toks = skills.map((s) => tokens(s.description));
  for (let i = 0; i < skills.length; i++) {
    for (let j = i + 1; j < skills.length; j++) {
      if (skills[i]!.name === skills[j]!.name) continue;
      const score = jaccard(toks[i]!, toks[j]!);
      if (score >= threshold)
        issues.push({
          kind: "near-duplicate",
          skill: skills[i]!.path,
          other: skills[j]!.path,
          detail: `description similarity ${score.toFixed(2)}`,
        });
    }
  }
  return issues;
}

export function defaultRoots(
  repo = REPO_ROOT,
): Array<{ dir: string; source: string }> {
  const roots: Array<{ dir: string; source: string }> = [
    { dir: join(repo, "skills"), source: "skills" },
  ];
  const pkgs = join(repo, "packages");
  if (existsSync(pkgs))
    for (const p of readdirSync(pkgs))
      roots.push({ dir: join(pkgs, p, "skills"), source: `packages/${p}` });
  const mk = join(repo, "plugins", "marketplaces");
  if (existsSync(mk))
    for (const m of readdirSync(mk)) {
      roots.push({ dir: join(mk, m), source: `marketplace/${m}` });
    }
  return roots;
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const rootsArg = args.indexOf("--roots");
  const roots =
    rootsArg >= 0
      ? args[rootsArg + 1]!.split(",").map((d) => ({ dir: d, source: d }))
      : defaultRoots().filter((r) => !r.source.startsWith("marketplace/"));
  const skills = roots.flatMap((r) => loadSkills(r.dir, r.source));
  const issues = audit(skills);
  const listingChars = skills
    .filter((s) => !s.hidden)
    .reduce(
      (n, s) => n + Math.min(s.description.length, 200) + s.name.length,
      0,
    );
  if (args.includes("--json")) {
    console.log(
      JSON.stringify({ skills: skills.length, listingChars, issues }, null, 2),
    );
  } else {
    console.log(
      `${skills.length} skills, listing cost about ${listingChars} chars (description capped at 200)`,
    );
    for (const i of issues)
      console.log(
        `${i.kind}: ${relative(REPO_ROOT, i.skill)}${i.other ? ` <-> ${relative(REPO_ROOT, i.other)}` : ""} - ${i.detail}`,
      );
  }
  process.exit(issues.some((i) => i.kind !== "near-duplicate") ? 1 : 0);
}
