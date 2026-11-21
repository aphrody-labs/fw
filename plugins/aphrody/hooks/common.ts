import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";

export type HookInput = {
  session_id?: string;
  cwd?: string;
  hook_event_name?: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
  tool_response?: unknown;
  prompt?: string;
  error?: string;
  [k: string]: unknown;
};

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
/** Whether this bun carries `bun uv` and `bun mcp`; probed once per binary (two spawns) and cached in the temp dir. */
export function bunCapabilities(): { uv: boolean; mcp: boolean } {
  if (cachedCaps) return cachedCaps;
  const key = `${process.execPath}|${Bun.version_with_sha}`;
  const cacheFile = join(tmpdir(), "aphrody-bun-capabilities.json");
  try {
    const cached = JSON.parse(readFileSync(cacheFile, "utf8")) as { key?: string; uv?: boolean; mcp?: boolean };
    if (cached.key === key && typeof cached.uv === "boolean" && typeof cached.mcp === "boolean") {
      return (cachedCaps = { uv: cached.uv, mcp: cached.mcp });
    }
  } catch {
    // No cache yet: probe below.
  }
  const url = (process.release as { sourceUrl?: string } | undefined)?.sourceUrl ?? "";
  const isFork = url.includes("aphrody-labs/bun");
  const probe = (command: string): boolean =>
    isFork || Bun.spawnSync([process.execPath, command, "--help"], { stdout: "ignore", stderr: "ignore", stdin: "ignore" }).exitCode === 0;
  cachedCaps = { uv: probe("uv"), mcp: probe("mcp") };
  try {
    writeFileSync(cacheFile, JSON.stringify({ key, ...cachedCaps }));
  } catch {
    // A read-only temp dir only costs the probe next time.
  }
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
