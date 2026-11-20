// SearXNG search and page reading for agents: mcp-searxng (searxng_web_search, searxng_search_suggestions,
// searxng_instance_info, web_url_read) started as a stdio MCP server, reached through the ai-os hub
// (hub_call searxng__<tool>) and registered by [[hub.mcp]] searxng in apps/agents/toolchain.toml.
// Every upstream feature stays available: pagination, filters, image search, PDF reading, sections, failover, caching;
// its settings are environment variables (SEARXNG_*), listed under [search.env] of ay.local.toml.
//   ay.local.toml  [search] url = "http://127.0.0.1:8888"   (or SEARXNG_URL)
//                  via = "test"                             (optional host role: run the server there over ssh)
//                  [search.env] SEARXNG_MAX_RESULTS = "20"
import { readAy } from "../../../../tools/local.ts";

type Json = Record<string, any>;
export interface Launch { argv: string[]; env: Record<string, string> }

const SAFE = /^[\w.:/@%+=,-]+$/;
const ROLES = ["build", "test", "prod"];

function checked(name: string, value: string): string {
  if (!SAFE.test(value)) throw new Error(`searxng: ${name} has characters outside [A-Za-z0-9_.:/@%+=,-]`);
  return value;
}

/** The command that starts mcp-searxng: locally on Bun, or over ssh on the host of role `via`. */
export function resolveLaunch(ay: Json = readAy(), env: Record<string, string | undefined> = process.env): Launch {
  const search: Json = ay.search ?? {};
  const url = env.SEARXNG_URL ?? search.url;
  if (!url) throw new Error("searxng: set SEARXNG_URL or [search] url in ay.local.toml");
  const parsed = new URL(url);
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") throw new Error("searxng: url must be http(s)");
  const settings: Record<string, string> = { SEARXNG_URL: checked("url", url) };
  for (const [k, v] of Object.entries((search.env ?? {}) as Json)) {
    if (!/^SEARXNG_[A-Z0-9_]+$/.test(k)) throw new Error(`searxng: [search.env] ${k} is not a SEARXNG_* setting`);
    settings[k] = checked(k, String(v));
  }
  const via = search.via as string | undefined;
  if (!via) return { argv: ["bun", "x", "--bun", "mcp-searxng"], env: settings };
  if (!ROLES.includes(via)) throw new Error(`searxng: via must be one of ${ROLES.join(", ")}`);
  const alias = ay.hosts?.[via];
  if (!alias) throw new Error(`searxng: [hosts].${via} is not set`);
  const remote = [...Object.entries(settings).map(([k, v]) => `${k}=${v}`), "bunx", "mcp-searxng"].join(" ");
  return { argv: ["ssh", "-o", "BatchMode=yes", "-o", "ServerAliveInterval=30", alias, remote], env: {} };
}

if (import.meta.main) {
  const { argv, env } = resolveLaunch();
  const p = Bun.spawn(argv, { stdin: "inherit", stdout: "inherit", stderr: "inherit", env: { ...process.env, ...env } });
  process.exit(await p.exited);
}
