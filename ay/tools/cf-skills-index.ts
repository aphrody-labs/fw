#!/usr/bin/env bun
// Regenerates the agent-skills discovery index (Cloudflare agent-skills RFC) served by apps/cf-gateway at
// /.well-known/agent-skills/index.json. The skills themselves are never copied here: the official Cloudflare ones belong to the
// Cloudflare plugin and the first-party ones to the aphrody plugin; the index only records their name, description and digest.
//   bun tools/cf-skills-index.ts [--skills <dir>]... [--check]
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

export const INDEX_PATH = resolve(import.meta.dir, "../apps/cf-gateway/src/agent-skills-index.json");
const SCHEMA = "https://cloudflare.github.io/agent-skills-discovery-rfc/schema.json";

export interface SkillEntry {
  name: string;
  description: string;
  url: string;
  sha256: string;
  bytes: number;
}

export function parseFrontmatter(md: string): { name?: string; description?: string } {
  const m = /^---\r?\n([\s\S]*?)\r?\n---/.exec(md);
  if (!m) return {};
  const out: Record<string, string> = {};
  for (const line of m[1]!.split(/\r?\n/)) {
    const kv = /^([a-zA-Z0-9_-]+):\s*(.+)$/.exec(line.trim());
    if (kv) out[kv[1]!] = kv[2]!.replace(/^["']|["']$/g, "");
  }
  return out;
}

export interface SkillSource {
  dir: string;
  /** Only these skills of the directory; every skill when absent. */
  only?: string[];
}

export function buildSkillsIndex(sources: SkillSource[], baseUrl = "/skills") {
  const byName = new Map<string, SkillEntry>();
  for (const { dir, only } of sources) {
    if (!existsSync(dir)) continue;
    for (const d of readdirSync(dir, { withFileTypes: true })) {
      if (only && !only.includes(d.name)) continue;
      const md = join(dir, d.name, "SKILL.md");
      if (!d.isDirectory() || !existsSync(md)) continue;
      const raw = readFileSync(md, "utf8");
      const fm = parseFrontmatter(raw);
      byName.set(d.name, {
        name: fm.name ?? d.name,
        description: fm.description ?? `Cloudflare ${d.name} skill`,
        url: `${baseUrl}/${d.name}/SKILL.md`,
        sha256: new Bun.CryptoHasher("sha256").update(raw).digest("hex"),
        bytes: new TextEncoder().encode(raw).byteLength,
      });
    }
  }
  const skills = [...byName.values()].sort((a, b) => a.name.localeCompare(b.name));
  return { $schema: SCHEMA, generated_at: new Date().toISOString(), skills };
}

/** Structural check of a published index: a schema, unique names, skill urls under /skills/, 64-hex digests. */
export function checkIndex(index: { $schema?: string; skills?: SkillEntry[] }): string[] {
  const errors: string[] = [];
  if (index.$schema !== SCHEMA) errors.push("index $schema is not the agent-skills discovery RFC schema");
  const seen = new Set<string>();
  for (const s of index.skills ?? []) {
    if (seen.has(s.name)) errors.push(`duplicate skill ${s.name}`);
    seen.add(s.name);
    if (!s.url.startsWith("/skills/")) errors.push(`${s.name}: url ${s.url} is not under /skills/`);
    if (!/^[0-9a-f]{64}$/.test(s.sha256)) errors.push(`${s.name}: sha256 is not a 64-hex digest`);
  }
  if (!seen.size) errors.push("index lists no skill");
  return errors;
}

if (import.meta.main) {
  const argv = Bun.argv.slice(2);
  if (argv.includes("--check")) {
    const errs = checkIndex(JSON.parse(readFileSync(INDEX_PATH, "utf8")));
    for (const e of errs) console.error(`INDEX FAIL: ${e}`);
    process.exit(errs.length ? 1 : 0);
  }
  const dirs = argv.flatMap((a, i) => (a === "--skills" && argv[i + 1] ? [resolve(argv[i + 1]!)] : []));
  const home = homedir();
  const sources: SkillSource[] = dirs.length
    ? dirs.map((dir) => ({ dir }))
    : [{ dir: join(home, ".gemini/plugins/cloudflare/skills") }, { dir: join(home, ".gemini/plugins/aphrody/skills"), only: ["ay-cf-finops-infra"] }];
  const index = buildSkillsIndex(sources);
  await Bun.write(INDEX_PATH, JSON.stringify(index, null, 2) + "\n");
  console.log(`Wrote ${INDEX_PATH} (${index.skills.length} skills)`);
}
