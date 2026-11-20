#!/usr/bin/env bun
// Hook latency benchmark. Cases are derived from the plugin hooks.json files, so a layout change is measured without editing this script.
//   bun scripts/bench.ts <label> [--runs N]      writes docs/bench/<label>.json
//   bun scripts/bench.ts compare <before> <after> prints a before/after table
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { discoverWorkspaceRoot, REPO_ROOT, runCmd, toSlash } from "./lib.ts";

export interface HookCase {
  id: string;
  plugin: string;
  event: string;
  matcher: string;
  command: string;
  payload: Record<string, unknown>;
}

export interface CaseResult {
  id: string;
  plugin: string;
  event: string;
  command: string;
  meanMs: number;
  medianMs: number;
  stddevMs: number;
  minMs: number;
  maxMs: number;
  runs: number;
}

export interface EventSummary {
  event: string;
  processes: number;
  /** Hooks of one event run in parallel: the slowest one gates the event. */
  wallMs: number;
  /** Sum of all hook processes (CPU cost per event). */
  totalMs: number;
}

interface HookEntry {
  type?: string;
  command?: string;
  hooks?: HookEntry[];
  matcher?: string;
}

const PLUGINS = ["aphrody", "bun"] as const;

function firstTool(matcher: string): string {
  const first = matcher.split("|")[0];
  return first && first !== "*" ? first : "Bash";
}

export function payloadFor(
  event: string,
  matcher: string,
  cwd: string,
): Record<string, unknown> {
  const base = { session_id: "bench-session", cwd, hook_event_name: event };
  const tool = firstTool(matcher);
  switch (event) {
    case "SessionStart":
      return { ...base, source: "startup" };
    case "UserPromptSubmit":
      return { ...base, prompt: "benchmark prompt" };
    case "PreToolUse":
      return {
        ...base,
        tool_name: tool,
        tool_input: {
          command: "ls",
          file_path: join(REPO_ROOT, "scripts", "lib.ts"),
        },
      };
    case "PostToolUse":
      return {
        ...base,
        tool_name: tool,
        tool_input: {
          command: "ls",
          file_path: join(REPO_ROOT, "scripts", "lib.ts"),
        },
        tool_response: { stdout: "ok", exitCode: 0 },
      };
    default:
      return base;
  }
}

/** Flattens every hook command of a Claude Code hooks.json (`{hooks: {Event: [{matcher?, hooks: [...]}]}}`). */
export function casesFromHooks(
  plugin: string,
  root: string,
  hooksJson: unknown,
  cwd: string,
): HookCase[] {
  const hooks =
    (hooksJson as { hooks?: Record<string, HookEntry[]> }).hooks ?? {};
  const out: HookCase[] = [];
  for (const [event, groups] of Object.entries(hooks)) {
    for (const group of groups) {
      const matcher = group.matcher ?? "";
      for (const h of group.hooks ?? []) {
        if (h.type !== "command" || !h.command) continue;
        const command = h.command.replaceAll(
          "${CLAUDE_PLUGIN_ROOT}",
          root.replaceAll("\\", "/"),
        );
        const script = command.match(/([\w.-]+)\.ts/)?.[1] ?? "hook";
        const mode = command.split(/\s+/).slice(-1)[0] ?? "";
        out.push({
          id: `${plugin}/${event}/${script}-${mode}`.toLowerCase(),
          plugin,
          event,
          matcher,
          command,
          payload: payloadFor(event, matcher, cwd),
        });
      }
    }
  }
  return out;
}

export function collectCases(repoRoot: string, cwd: string): HookCase[] {
  const cases: HookCase[] = [];
  for (const plugin of PLUGINS) {
    const root = join(repoRoot, "packages", plugin);
    const file = join(root, ".claude-plugin", "hooks.json");
    if (existsSync(file))
      cases.push(
        ...casesFromHooks(
          plugin,
          root,
          JSON.parse(readFileSync(file, "utf8")),
          cwd,
        ),
      );
  }
  return cases;
}

export function summarizeEvents(results: CaseResult[]): EventSummary[] {
  const byEvent = new Map<string, CaseResult[]>();
  for (const r of results)
    byEvent.set(r.event, [...(byEvent.get(r.event) ?? []), r]);
  return [...byEvent].map(([event, rs]) => ({
    event,
    processes: rs.length,
    wallMs: Math.max(...rs.map((r) => r.meanMs)),
    totalMs: rs.reduce((s, r) => s + r.meanMs, 0),
  }));
}

/** hyperfine 1.x exports flat seconds; 2.x (schema_version 2) nests them under summary.time_wall_clock. */
export interface HyperfineResult {
  command: string;
  mean?: number;
  median?: number;
  stddev?: number;
  min?: number;
  max?: number;
  times?: number[];
  summary?: {
    time_wall_clock: {
      count: number;
      mean: number;
      median: number;
      stddev: number;
      min: number;
      max: number;
    };
  };
}

export function fromHyperfine(c: HookCase, r: HyperfineResult): CaseResult {
  const w = r.summary?.time_wall_clock ?? {
    count: r.times?.length ?? 0,
    mean: r.mean ?? 0,
    median: r.median ?? 0,
    stddev: r.stddev ?? 0,
    min: r.min ?? 0,
    max: r.max ?? 0,
  };
  const ms = (s: number) => Math.round(s * 1000 * 100) / 100;
  return {
    id: c.id,
    plugin: c.plugin,
    event: c.event,
    command: c.command,
    meanMs: ms(w.mean),
    medianMs: ms(w.median),
    stddevMs: ms(w.stddev),
    minMs: ms(w.min),
    maxMs: ms(w.max),
    runs: w.count,
  };
}

function bench(c: HookCase, dir: string, runs: number): CaseResult {
  const input = join(dir, `${c.id.replaceAll("/", "_")}.json`);
  const out = `${input}.result`;
  writeFileSync(input, JSON.stringify(c.payload));
  const r = runCmd([
    "hyperfine",
    "-N",
    "--warmup",
    "3",
    "--runs",
    String(runs),
    "--input",
    input,
    "--export-json",
    out,
    c.command,
  ]);
  if (!r.ok) throw new Error(`hyperfine failed for ${c.id}: ${r.err || r.out}`);
  const parsed = JSON.parse(readFileSync(out, "utf8")) as {
    results: HyperfineResult[];
  };
  return fromHyperfine(c, parsed.results[0]!);
}

function baseline(runs: number): CaseResult {
  const r = runCmd([
    "hyperfine",
    "-N",
    "--warmup",
    "3",
    "--runs",
    String(runs),
    "--export-json",
    join(tmpdir(), "bench-baseline.json"),
    "bun --no-install --no-env-file -e 0",
  ]);
  if (!r.ok) throw new Error(`hyperfine failed: ${r.err}`);
  const parsed = JSON.parse(
    readFileSync(join(tmpdir(), "bench-baseline.json"), "utf8"),
  ) as { results: HyperfineResult[] };
  return fromHyperfine(
    {
      id: "bun-startup",
      plugin: "runtime",
      event: "none",
      matcher: "",
      command: "bun -e 0",
      payload: {},
    },
    parsed.results[0]!,
  );
}

export function compareTable(before: BenchFile, after: BenchFile): string {
  const rows = [
    "| event | before wall ms | after wall ms | before procs | after procs | before total ms | after total ms |",
    "|---|---|---|---|---|---|---|",
  ];
  const a = new Map(after.events.map((e) => [e.event, e]));
  for (const b of before.events) {
    const x = a.get(b.event);
    rows.push(
      `| ${b.event} | ${b.wallMs.toFixed(1)} | ${x ? x.wallMs.toFixed(1) : "-"} | ${b.processes} | ${x?.processes ?? "-"} | ${b.totalMs.toFixed(1)} | ${x ? x.totalMs.toFixed(1) : "-"} |`,
    );
  }
  return rows.join("\n");
}

/** Key scripts measured next to the hooks (commands relative to the repo root). */
export const SCRIPT_CASES: Array<{
  id: string;
  command: string;
  payload?: Record<string, unknown>;
}> = [
  {
    id: "statusline",
    command: "bun --no-install --no-env-file scripts/statusline.ts",
    payload: {
      model: { display_name: "Sonnet" },
      cwd: discoverWorkspaceRoot(),
      context_window: { used_percentage: 42 },
    },
  },
  {
    id: "memory-search",
    command:
      "bun --no-install --no-env-file scripts/memory.ts search fleet roles",
  },
  {
    id: "cron-list",
    command: "bun --no-install --no-env-file scripts/cron.ts list",
  },
  {
    id: "skills-audit",
    command: "bun --no-install --no-env-file scripts/skills.ts",
  },
  { id: "graph", command: "bun --no-install --no-env-file scripts/graph.ts" },
  {
    id: "matrix-check",
    command: "bun --no-install --no-env-file scripts/matrix.ts --check",
  },
];

export interface BenchFile {
  label: string;
  date: string;
  bun: string;
  hyperfine: string;
  baseline: CaseResult;
  cases: CaseResult[];
  events: EventSummary[];
  scripts?: CaseResult[];
}

const benchDir = join(REPO_ROOT, "docs", "bench");

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  if (args[0] === "compare") {
    const read = (l: string) =>
      JSON.parse(
        readFileSync(join(benchDir, `${l}.json`), "utf8"),
      ) as BenchFile;
    console.log(compareTable(read(args[1]!), read(args[2]!)));
    process.exit(0);
  }
  const label = args[0];
  if (!label) {
    console.error(
      "usage: bun scripts/bench.ts <label> [--runs N] | compare <before> <after>",
    );
    process.exit(1);
  }
  const runsIdx = args.indexOf("--runs");
  const runs = runsIdx >= 0 ? Number(args[runsIdx + 1]) : 15;
  const tmp = mkdtempSync(join(tmpdir(), "bench-"));
  try {
    const cases = collectCases(REPO_ROOT, tmp);
    const results = cases.map((c) => bench(c, tmp, runs));
    const file: BenchFile = {
      label,
      date: new Date().toISOString(),
      bun: Bun.version,
      hyperfine: runCmd(["hyperfine", "--version"]).out,
      baseline: baseline(runs),
      cases: results,
      events: summarizeEvents(results),
      scripts: SCRIPT_CASES.map((c) =>
        bench(
          {
            id: `script/${c.id}`,
            plugin: "scripts",
            event: "script",
            matcher: "",
            command: c.command,
            payload: c.payload ?? {},
          },
          tmp,
          runs,
        ),
      ),
    };
    mkdirSync(benchDir, { recursive: true });
    writeFileSync(
      join(benchDir, `${label}.json`),
      `${JSON.stringify(file, null, 2).replaceAll(toSlash(REPO_ROOT), "~/.claude")}\n`,
    );
    console.log(`bun startup ${file.baseline.meanMs} ms`);
    for (const x of file.scripts ?? []) console.log(`${x.id}: ${x.meanMs} ms`);
    for (const e of file.events)
      console.log(
        `${e.event}: ${e.processes} processes, wall ${e.wallMs.toFixed(1)} ms, total ${e.totalMs.toFixed(1)} ms`,
      );
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}
