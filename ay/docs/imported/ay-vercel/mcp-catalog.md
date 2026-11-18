# Vercel MCP Servers & Tool Catalog

`ay-vercel` configures and validates both client MCP endpoints (`.mcp.json`, `mcp_config.json`) and server-side MCP handlers (`apps/web/src/app/api/mcp/route.ts`).

## Configured Endpoints

| Server | Transport | Target | Auth | Source |
| :--- | :--- | :--- | :--- | :--- |
| `vercel` | Streamable HTTP | `https://mcp.vercel.com` | OAuth 2.1 + CIMD | `vercel/vercel-mcp-overview` |
| `next-devtools` | Stdio | `bunx next-devtools-mcp@latest` | Local dev server | `vercel/next-devtools-mcp` |

## Server SDKs & Tool Index

- Full declarative catalog: [`mcp/catalog.toml`](../mcp/catalog.toml)
- Indexed Vercel MCP tool schemas (`251` tools): [`mcp/vercel-tools-index.json`](../mcp/vercel-tools-index.json)
- Local Next.js App Router Streamable HTTP MCP route (`mcp-handler`): [`apps/web/src/app/api/mcp/route.ts`](../apps/web/src/app/api/mcp/route.ts)

Refresh and verify via:

```sh
just mcp-sync
```
