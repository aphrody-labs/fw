import { existsSync, readFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";

export type Harness = "claude" | "agy" | "gemini";

export type HookInput = {
  session_id?: string;
  conversationId?: string;
  cwd?: string;
  workspacePaths?: string[];
  hook_event_name?: string;
  hookEventName?: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
  toolCall?: {
    id?: string;
    name?: string;
    args?: Record<string, unknown>;
  };
  tool_response?: unknown;
  prompt?: string;
  user_prompt?: string;
  lastUserInput?: string;
  error?: string;
  [k: string]: unknown;
};

export function detectHarness(arg?: string): Harness {
  if (arg === "agy" || arg === "gemini" || arg === "claude") return arg;
  if (process.env.CLAUDE_PLUGIN_ROOT) return "claude";
  if (process.env.GEMINI_CLI || process.env.extensionPath) return "gemini";
  return "claude";
}

export async function readHookInput(): Promise<HookInput> {
  if (process.stdin.isTTY) return {};
  try {
    const text = await new Response(Bun.stdin.stream()).text();
    if (!text.trim()) return {};
    return JSON.parse(text) as HookInput;
  } catch {
    return {};
  }
}

export function writeJson(data: unknown): void {
  process.stdout.write(JSON.stringify(data));
}

export function isBunCheckout(cwd?: string): boolean {
  let dir = resolve(cwd || process.cwd());
  for (let i = 0; i < 8; i++) {
    const pkg = join(dir, "package.json");
    if (existsSync(pkg)) {
      try {
        const parsed = JSON.parse(readFileSync(pkg, "utf8")) as {
          name?: string;
        };
        if (parsed.name === "bun" && existsSync(join(dir, "src", "bun.js")))
          return true;
      } catch {
        // Ignore malformed package.json.
      }
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return false;
}

let cachedCaps: { uv: boolean; mcp: boolean } | undefined;
export function bunCapabilities(): { uv: boolean; mcp: boolean } {
  if (cachedCaps) return cachedCaps;
  const url =
    (process.release as { sourceUrl?: string } | undefined)?.sourceUrl ?? "";
  const isFork = url.includes("aphrody-labs/bun");
  const hasMcp =
    isFork ||
    Bun.spawnSync([process.execPath, "mcp", "--help"], {
      stdout: "ignore",
      stderr: "ignore",
      stdin: "ignore",
    }).exitCode === 0;
  const hasUv =
    isFork ||
    Bun.spawnSync([process.execPath, "uv", "--help"], {
      stdout: "ignore",
      stderr: "ignore",
      stdin: "ignore",
    }).exitCode === 0;
  cachedCaps = { uv: hasUv, mcp: hasMcp };
  return cachedCaps;
}

export function isAphrodyBun(): boolean {
  return bunCapabilities().uv;
}

export type PreferRule = { instead: string; bash: string; use: string[] };
export type ToolsCatalog = { tools: string[]; prefer: PreferRule[] };

export function loadToolsCatalog(): ToolsCatalog {
  const p = join(import.meta.dir, "tools.json");
  return JSON.parse(readFileSync(p, "utf8")) as ToolsCatalog;
}

export function preferHint(command: string): string | undefined {
  if (!bunCapabilities().mcp) return undefined;
  const catalog = loadToolsCatalog();
  for (const rule of catalog.prefer) {
    if (new RegExp(rule.bash).test(command)) {
      return `Prefer bun MCP tools (${rule.use.join(", ")}) over ${basename(rule.instead)} when inspecting project code.`;
    }
  }
  return undefined;
}
