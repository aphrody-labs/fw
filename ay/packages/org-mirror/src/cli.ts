#!/usr/bin/env bun
/** `org-mirror`: replicate a GitHub organization into bun:sqlite, then browse it. Every report goes to stdout, progress to stderr. */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { parseArgs } from "node:util";
import { defaultDbPath, openDb, dataRoot } from "./db.ts";
import { exportToml, withKept } from "./export.ts";
import { GitHub, discoverToken } from "./github.ts";
import { serve } from "./server.ts";
import { formatStats, stats } from "./stats.ts";
import { STEPS, syncOrg, type Step } from "./sync.ts";

const USAGE = `usage: org-mirror <command> <org> [options]
  sync <org>    [--repo NAME]... [--top N] [--since ISO] [--forks] [--package npm:NAME]... [--steps ${STEPS.join(",")}] [--concurrency N] [--db PATH]
  stats <org>   row counts per table and the replica against GitHub's totals [--json]
  export <org>  --toml [--out PATH]   research dataset summary (default docs/research/<org>.toml in the repository)
  serve <org>   [--port 8788] [--host 127.0.0.1]   browse the replica
defaults: --db ${defaultDbPath("<org>")}; data root from AY_ORG_MIRROR_DIR, else AY_DATA_DIR/mirror; token from GH_TOKEN, GITHUB_TOKEN or \`gh auth token\``;

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    repo: { type: "string", multiple: true },
    top: { type: "string" },
    since: { type: "string" },
    forks: { type: "boolean" },
    steps: { type: "string" },
    package: { type: "string", multiple: true },
    concurrency: { type: "string" },
    db: { type: "string" },
    json: { type: "boolean" },
    toml: { type: "boolean" },
    out: { type: "string" },
    port: { type: "string" },
    host: { type: "string" },
    help: { type: "boolean", short: "h" },
  },
});

const [command, org] = positionals;
if (values.help || !command || !org) {
  console.log(USAGE);
  process.exit(values.help ? 0 : 2);
}

const dbPath = values.db ?? defaultDbPath(org);
const log = (line: string) => console.error(`${new Date().toISOString().slice(11, 19)} ${line}`);

/** The repository root: the nearest parent holding ay.toml. */
function repoRoot(): string {
  for (let d = import.meta.dir; ; d = dirname(d)) {
    if (existsSync(join(d, "ay.toml"))) return d;
    if (dirname(d) === d) throw new Error("ay.toml not found above " + import.meta.dir);
  }
}

if (command === "sync") {
  const steps = values.steps?.split(",").map(s => s.trim()) ?? [...STEPS];
  const bad = steps.filter(s => !(STEPS as readonly string[]).includes(s));
  if (bad.length) throw new Error(`unknown step(s): ${bad.join(", ")}`);
  const token = discoverToken();
  if (!token) log("no GitHub token found: unauthenticated GraphQL is not possible, set GH_TOKEN");
  const db = openDb(dbPath);
  const report = await syncOrg({
    org, db, gh: new GitHub({ token, log }), root: dataRoot(), repos: values.repo, top: values.top ? Number(values.top) : undefined, since: values.since, forks: values.forks,
    packages: values.package, steps: steps as Step[], concurrency: values.concurrency ? Number(values.concurrency) : undefined, token, log,
  });
  console.log(JSON.stringify(report, null, 2));
  process.exit(report.errors.length ? 1 : 0);
} else if (command === "stats") {
  const s = stats(openDb(dbPath), org);
  console.log(values.json ? JSON.stringify(s, null, 2) : formatStats(s));
} else if (command === "export") {
  if (!values.toml) throw new Error("export needs --toml");
  const out = values.out ?? join(repoRoot(), "docs", "research", `${org}.toml`);
  const text = withKept(exportToml(openDb(dbPath), org), existsSync(out) ? readFileSync(out, "utf8") : undefined);
  writeFileSync(out, text);
  console.error(`wrote ${out} (${text.length} bytes)`);
} else if (command === "serve") {
  const server = serve(openDb(dbPath), org, { port: Number(values.port ?? 8788), hostname: values.host ?? "127.0.0.1" });
  console.log(`org-mirror ${org} listening on ${server.url}`);
} else {
  console.log(USAGE);
  process.exit(2);
}
