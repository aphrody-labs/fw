// Bun side of `fw crates`: the plugin command and the MCP tools both run the Rust engine (the `fw-crates` binary,
// the same one `fw crates` spawns), so there is one ranking, one dataset and one set of output formats.
import { existsSync } from "node:fs";
import { join } from "node:path";

const exe = (name: string): string => (process.platform === "win32" ? `${name}.exe` : name);

/** Finds `fw-crates` next to a built `fw`, then in dist/, then on PATH; undefined when it has not been built. */
export function crateEngine(root: string): string | undefined {
  const targetDir = process.env.CARGO_TARGET_DIR || join(root, "target");
  const candidates = [
    join(targetDir, "release", exe("fw-crates")),
    join(root, "target", "release", exe("fw-crates")),
    join(root, "dist", exe("fw-crates")),
  ];
  return candidates.find((p) => existsSync(p)) ?? Bun.which("fw-crates") ?? undefined;
}

export const BUILD_HINT = "fw-crates is not built: run `cargo build --release -p fw-crates` (it sits next to `fw`) or put it on PATH";

export interface EngineResult {
  ok: boolean;
  code: number;
  stdout: string;
  stderr: string;
}

/** Runs the engine with `argv` and captures its output. */
export function runEngine(root: string, argv: string[]): EngineResult {
  const bin = crateEngine(root);
  if (!bin) return { ok: false, code: 127, stdout: "", stderr: BUILD_HINT };
  const proc = Bun.spawnSync([bin, ...argv], { cwd: root, stdout: "pipe", stderr: "pipe", env: { ...process.env } });
  const code = proc.exitCode ?? 1;
  return { ok: code === 0, code, stdout: proc.stdout.toString(), stderr: proc.stderr.toString() };
}

/** `fw plugin` command: `crates <search|info|compare|rank|awesome|bench|stack|sync> ...`, streaming to the terminal. */
export function cmdCrates(root: string, argv: string[]): number {
  const bin = crateEngine(root);
  if (!bin) {
    console.error(JSON.stringify({ ok: false, error: BUILD_HINT }));
    return 127;
  }
  const proc = Bun.spawnSync([bin, ...argv], { cwd: root, stdout: "inherit", stderr: "inherit", stdin: "inherit", env: { ...process.env } });
  return proc.exitCode ?? 1;
}

type Args = Record<string, unknown>;

const text = (v: unknown): string | undefined => (typeof v === "string" && v.trim() ? v.trim() : undefined);

/** CLI arguments (always `--json`) for an MCP tool call, or the reason a required argument is missing. */
export function toolArgv(name: string, args: Args): string[] | string {
  const limit = typeof args.limit === "number" ? ["--limit", String(Math.max(1, Math.trunc(args.limit)))] : [];
  const live = args.live === true ? [] : ["--offline"];
  switch (name) {
    case "crates_search": {
      const q = text(args.query);
      if (!q) return "missing `query`";
      const cat = text(args.category);
      return ["search", q, ...(cat ? ["--category", cat] : []), ...limit, "--json"];
    }
    case "crates_info": {
      const n = text(args.name);
      return n ? ["info", n, ...live, "--json"] : "missing `name`";
    }
    case "crates_compare": {
      const names = Array.isArray(args.names) ? args.names.filter((n): n is string => typeof n === "string" && n.trim() !== "") : [];
      return names.length >= 2 ? ["compare", ...names, ...live, "--json"] : "compare needs at least two crate names";
    }
    case "crates_awesome": {
      const cat = text(args.category);
      return ["awesome", ...(cat ? [cat] : []), ...limit, "--json"];
    }
    case "crates_stack": {
      const goal = text(args.goal);
      return goal ? ["stack", goal, "--json"] : "missing `goal`";
    }
    default:
      return `unknown crates tool: ${name}`;
  }
}

export const CRATES_TOOLS = [
  {
    name: "crates_search",
    description:
      "Search the ranked crates.io dataset by words (name, keywords, categories, description); results ordered by relevance then score.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string" },
        category: { type: "string", description: "crates.io category slug or unique part of it" },
        limit: { type: "integer" },
      },
      required: ["query"],
    },
  },
  {
    name: "crates_info",
    description:
      "One crate: score with its components, downloads, dependents, releases, MSRV, licence, advisories, rank overall and per category; live=true adds the live sparse-index record.",
    inputSchema: { type: "object", properties: { name: { type: "string" }, live: { type: "boolean" } }, required: ["name"] },
  },
  {
    name: "crates_compare",
    description: "Compare two or more crates side by side with the best crate per metric.",
    inputSchema: {
      type: "object",
      properties: { names: { type: "array", items: { type: "string" } }, live: { type: "boolean" } },
      required: ["names"],
    },
  },
  {
    name: "crates_awesome",
    description: "The ranked best crates of a crates.io category, or the index of every awesome list when no category is given.",
    inputSchema: { type: "object", properties: { category: { type: "string" }, limit: { type: "integer" } } },
  },
  {
    name: "crates_stack",
    description:
      "Pick crates from the ranking for a goal in words (for example `web API with Postgres and auth`) and return the choices with reasons and a Cargo project skeleton.",
    inputSchema: { type: "object", properties: { goal: { type: "string" } }, required: ["goal"] },
  },
] as const;

export const CRATES_TOOL_NAMES: string[] = CRATES_TOOLS.map((t) => t.name);

/** MCP tool call: runs the engine and returns its JSON (parsed) or an error. */
export function callCratesTool(root: string, name: string, args: Args): { ok: boolean; payload: unknown } {
  const argv = toolArgv(name, args);
  if (typeof argv === "string") return { ok: false, payload: { ok: false, error: argv } };
  const res = runEngine(root, argv);
  if (!res.ok) return { ok: false, payload: { ok: false, error: res.stderr.trim() || `fw-crates exited ${res.code}` } };
  try {
    return { ok: true, payload: JSON.parse(res.stdout) };
  } catch {
    return { ok: true, payload: res.stdout };
  }
}
