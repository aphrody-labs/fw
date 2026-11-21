// Bun side of `fw kernel`: the MCP tools run the Rust engine (the `fw-linux` binary that `fw kernel` spawns), so the
// pins, the releases comparison and the code graph have one implementation and one JSON shape.
import { existsSync } from "node:fs";
import { join } from "node:path";

const exe = (name: string): string => (process.platform === "win32" ? `${name}.exe` : name);

/** Finds `fw-linux`: $FW_LINUX_BIN, then the cargo target directories and dist/, then PATH. */
export function kernelEngine(root: string): string | undefined {
  const fromEnv = process.env.FW_LINUX_BIN;
  if (fromEnv && existsSync(fromEnv)) return fromEnv;
  const targetDirs = [process.env.CARGO_TARGET_DIR, join(root, "target")].filter((dir): dir is string => Boolean(dir));
  const candidates = [
    ...targetDirs.flatMap((dir) => [join(dir, "release", exe("fw-linux")), join(dir, "debug", exe("fw-linux"))]),
    join(root, "dist", exe("fw-linux")),
  ];
  return candidates.find((path) => existsSync(path)) ?? Bun.which("fw-linux") ?? undefined;
}

export const KERNEL_BUILD_HINT = "fw-linux is not built: run `cargo build --release -p fw-linux` (it sits next to `fw`), put it on PATH or set FW_LINUX_BIN";

/** Queries `fw kernel graph` answers; `build` is a CLI action, not a tool. */
export const GRAPH_QUERIES = ["stats", "def", "refs", "callers", "callees", "includes", "included-by", "file", "config", "search", "path"] as const;

type Args = Record<string, unknown>;

const text = (value: unknown): string | undefined => (typeof value === "string" && value.trim() ? value.trim() : undefined);

/** CLI arguments for an MCP tool call, or the reason a required argument is missing. */
export function kernelArgv(name: string, args: Args): string[] | string {
  switch (name) {
    case "kernel_status":
      return ["status", ...(args.offline === true ? ["--offline"] : []), "--json"];
    case "kernel_graph": {
      const query = text(args.query);
      if (!query || !(GRAPH_QUERIES as readonly string[]).includes(query)) return `\`query\` must be one of ${GRAPH_QUERIES.join(", ")}`;
      const operands = Array.isArray(args.operands) ? args.operands.filter((item): item is string => typeof item === "string" && item !== "") : [];
      if (query !== "stats" && operands.length === 0) return `${query} needs \`operands\` (a symbol, a file or a CONFIG name)`;
      const tree = text(args.tree);
      const limit = typeof args.limit === "number" ? ["--limit", String(Math.max(1, Math.trunc(args.limit)))] : [];
      return ["graph", query, ...operands, ...(tree ? ["--tree", tree] : []), ...limit];
    }
    default:
      return `unknown kernel tool: ${name}`;
  }
}

export const KERNEL_TOOLS = [
  {
    name: "kernel_status",
    description:
      "Linux kernel pins (mainline, stable, CachyOS and its PKGBUILDs from kernel.toml) against kernel.org releases.json (mainline, stable, longterm, linux-next) and the newest CachyOS tags: behind and inconsistent pins, gitlinks, remote tags.",
    inputSchema: { type: "object", properties: { offline: { type: "boolean", description: "Skip the network: only kernel.toml and the git index." } } },
  },
  {
    name: "kernel_graph",
    description:
      "Query the tree-sitter code graph of a pinned Linux kernel tree (symbols, definitions, references, includes, call edges, Kconfig and Kbuild links) with file:line answers. Build it first with `fw kernel graph build` on a host that has the tree.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", enum: [...GRAPH_QUERIES] },
        operands: { type: "array", items: { type: "string" }, description: "Symbol, file path or CONFIG_ name (two symbols for `path`)." },
        tree: { type: "string", description: "Pinned tree id: stable (default), mainline, cachyos." },
        limit: { type: "integer" },
      },
      required: ["query"],
    },
  },
] as const;

export const KERNEL_TOOL_NAMES: string[] = KERNEL_TOOLS.map((tool) => tool.name);

/** MCP tool call: runs the engine and returns its JSON (parsed) or an error. */
export function callKernelTool(root: string, name: string, args: Args): { ok: boolean; payload: unknown } {
  const argv = kernelArgv(name, args);
  if (typeof argv === "string") return { ok: false, payload: { ok: false, error: argv } };
  const bin = kernelEngine(root);
  if (!bin) return { ok: false, payload: { ok: false, error: KERNEL_BUILD_HINT } };
  const proc = Bun.spawnSync([bin, ...argv, "--root", root], { cwd: root, stdout: "pipe", stderr: "pipe", env: { ...process.env } });
  const stdout = proc.stdout.toString();
  // `status --check` style exits are not used here, so any non-zero exit is a failure of the call itself.
  if (proc.exitCode !== 0) return { ok: false, payload: { ok: false, error: proc.stderr.toString().trim() || `fw-linux exited ${proc.exitCode}` } };
  try {
    return { ok: true, payload: JSON.parse(stdout) };
  } catch {
    return { ok: true, payload: stdout };
  }
}
