#!/usr/bin/env bun
// Renders the feature matrix of docs/research/agents.toml into docs/research/agents.md and verifies the proof paths
// of the claude column against the repo.
//   bun scripts/matrix.ts          rewrite the matrix block of agents.md
//   bun scripts/matrix.ts --check  exit 1 when a claude proof path is missing or the block is stale
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { REPO_ROOT } from "./lib.ts";

export type Status = "present" | "partial" | "missing";
export const COLUMNS = ["openclaw", "hermes", "claude"] as const;
type Column = (typeof COLUMNS)[number];

export interface Cell {
  status: Status;
  proof: string;
  was?: Status;
  note?: string;
}

export interface Capability {
  id: string;
  area: string;
  capability: string;
  openclaw: Cell;
  hermes: Cell;
  claude: Cell;
}

export const START = "<!-- matrix:start -->";
export const END = "<!-- matrix:end -->";

export function parseMatrix(toml: string): Capability[] {
  const raw = Bun.TOML.parse(toml) as { capability?: Capability[] };
  const rows = raw.capability ?? [];
  const ids = new Set<string>();
  for (const r of rows) {
    if (ids.has(r.id)) throw new Error(`duplicate capability id ${r.id}`);
    ids.add(r.id);
    for (const c of COLUMNS) {
      if (!["present", "partial", "missing"].includes(r[c]?.status))
        throw new Error(`${r.id}.${c}: invalid status`);
      if (!r[c].proof) throw new Error(`${r.id}.${c}: proof required`);
    }
  }
  return rows;
}

/** The path part of a proof string: text before the first " (" or ",", with `none` meaning no path. */
export function proofPath(proof: string): string | null {
  const p = proof.split(/ \(|,/)[0]!.trim();
  return p === "none" ? null : p;
}

/** Directories that exist only on a machine that ran Claude Code (untracked runtime state): a proof inside them cannot exist in a clean checkout. */
const RUNTIME_DIRS = ["plugins", "projects"];

export function missingProofs(rows: Capability[], root = REPO_ROOT): string[] {
  const out: string[] = [];
  const tracked =
    existsSync(join(root, "projects")) || existsSync(join(root, "plugins"));
  for (const r of rows) {
    if (r.claude.status === "missing") continue;
    const p = proofPath(r.claude.proof);
    if (p && !tracked && RUNTIME_DIRS.includes(p.split("/")[0] as string))
      continue;
    if (p && !existsSync(join(root, p))) out.push(`${r.id}: ${p}`);
  }
  return out;
}

export function summarize(
  rows: Capability[],
): Record<Column, Record<Status, number>> {
  const out = {} as Record<Column, Record<Status, number>>;
  for (const c of COLUMNS) {
    out[c] = { present: 0, partial: 0, missing: 0 };
    for (const r of rows) out[c][r[c].status]++;
  }
  return out;
}

const cell = (c: Cell, code = true) =>
  `${c.status}${c.was && c.was !== c.status ? ` (was ${c.was})` : ""}: ${code ? `\`${c.proof}\`` : c.proof}`.replaceAll(
    "|",
    "\\|",
  );

export function renderMatrix(rows: Capability[]): string {
  const sum = summarize(rows);
  const lines = [
    START,
    "",
    "| area | capability | openclaw | hermes | claude repo today |",
    "|---|---|---|---|---|",
  ];
  for (const r of rows)
    lines.push(
      `| ${r.area} | ${r.capability} | ${cell(r.openclaw)} | ${cell(r.hermes)} | ${cell(r.claude)}${r.claude.note ? ` - ${r.claude.note.replaceAll("|", "\\|")}` : ""} |`,
    );
  lines.push(
    "",
    `Totals over ${rows.length} capabilities (present / partial / missing): ` +
      COLUMNS.map(
        (c) => `${c} ${sum[c].present}/${sum[c].partial}/${sum[c].missing}`,
      ).join(", ") +
      ".",
  );
  const closed = rows.filter(
    (r) => r.claude.was && r.claude.was !== r.claude.status,
  );
  lines.push(
    "",
    `Closed or improved on the claude side by this work (${closed.length}): ${closed.map((r) => `${r.id} (${r.claude.was} to ${r.claude.status})`).join(", ")}.`,
  );
  lines.push("", END);
  return lines.join("\n");
}

export function inject(doc: string, block: string): string {
  const a = doc.indexOf(START);
  const b = doc.indexOf(END);
  if (a < 0 || b < 0) throw new Error("matrix markers not found");
  return doc.slice(0, a) + block + doc.slice(b + END.length);
}

if (import.meta.main) {
  const dir = join(REPO_ROOT, "docs", "research");
  const rows = parseMatrix(readFileSync(join(dir, "agents.toml"), "utf8"));
  const docPath = join(dir, "agents.md");
  const doc = readFileSync(docPath, "utf8");
  const next = inject(doc, renderMatrix(rows));
  if (Bun.argv.includes("--check")) {
    const bad = missingProofs(rows);
    for (const b of bad) console.error(`missing proof path: ${b}`);
    if (next !== doc)
      console.error(
        "agents.md matrix block is stale: run bun scripts/matrix.ts",
      );
    process.exit(bad.length || next !== doc ? 1 : 0);
  }
  writeFileSync(docPath, next);
  console.log(`agents.md updated: ${rows.length} capabilities`);
}
