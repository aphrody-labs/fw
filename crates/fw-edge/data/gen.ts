// Regenerates the JSON tables the Rust fw-edge embeds from the TypeScript Worker (cloudflare/worker.ts).
// Usage: bun crates/fw-edge/data/gen.ts   (the tables are checked in; this script is the provenance)
import worker, { CLOUDFLARE_EDGE_OS_CATALOG, FwOSAgent } from "../../../cloudflare/worker.ts";

const out = (name: string, value: unknown) => Bun.write(new URL(name, import.meta.url), `${JSON.stringify(value, null, 2)}\n`);
const call = async (path: string, init?: RequestInit) => (await worker.fetch(new Request(`https://edge.fw.test${path}`, init), {})).json() as Promise<any>;

await out("catalog.json", CLOUDFLARE_EDGE_OS_CATALOG);
await out("tiers.json", (await call("/os/tiers")).tiers);
await out(
  "mcp_tools.json",
  (await call("/mcp", { method: "POST", body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" }) })).result.tools,
);
const spec = await call("/openapi.json");
await out("openapi_paths.json", spec.paths);
await out("openapi_description.json", spec.info.description);
await out(
  "gatekeeper_rules.json",
  new FwOSAgent().gatekeeperListPolicies().map(({ updatedAt: _ts, ...rule }) => rule),
);
