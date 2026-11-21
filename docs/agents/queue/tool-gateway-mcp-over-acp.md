# Lot: one in-process tool gateway, MCP-over-ACP backed by UTCP and rmcp

Owner's design, 2026-11-21: instead of choosing between MCP (a server between the agent and every API) and UTCP (a JSON manual and a direct call, with no server), fw hosts one tool router in memory, inside its ACP host and proxy.

The design comes from the video "MCP vs UTCP" (youtube.com/watch?v=7VfKiaGOZcA).

## Crates, checked live on crates.io on 2026-11-21

| Crate | Live version | Note |
|---|---|---|
| `rs-utcp` | 0.3.2 | updated 2026-03-05, repo universal-tool-calling-protocol/rs-utcp |
| `rmcp`, `rmcp-macros` | 3.11.0 | updated 2026-11-17, repo modelcontextprotocol/rust-sdk |
| `agent-client-protocol`, `agent-client-protocol-rmcp` | 5.4.0 | the design said 5.3; features of agent-client-protocol-rmcp: `rmcp_3`, `unstable_mcp_over_acp` |
| `agent-client-protocol-schema` | 1.20.0 | the design said 1.34.0, which is not on crates.io: re-check the repo tags before pinning |

## Architecture

1. **Tool calls:**
   - The agent speaks ACP to the fw host.
   - The host exposes, through `McpServer::from_rmcp`, one rmcp server per session, with its own serverId.
2. **Direct calls:** UTCP calls APIs directly (HTTP, gRPC, CLI, GraphQL, SSE…), using `OpenApiConverter` for APIs that publish OpenAPI.
3. **Existing MCP servers:** they are consumed through an rmcp 3.11 client (`ClientLifecycleMode::Auto`, MCP 2026-07-28 and 2025-11-25), registered as a UTCP communication protocol.
4. **Secrets:** they stay on the host (rs-utcp variable loaders or the fw vault). The agent never sees tokens.
5. **Context budget:**
   - Meta-tools `search_tools` (`TagSearchStrategy`), `call_tool` and `run_codemode` (`CodeModeUtcp`, Rhai sandbox).
   - Typed first-level tools stay available for clients that need them, through a manual `list_tools`/`call_tool` that projects the UTCP repository.
6. **Errors:** business errors return `Ok(CallToolResult::error)` (`isError: true`); `Err(McpError)` is only for JSON-RPC protocol errors.
7. **Protocols:** `allowed_communication_protocols` is always set in manuals, so a third-party manual cannot run `cli` or `text`.

## Fit with fw

fw already has its own MCP engine (`crates/fw-proto/src/mcp.rs`, 152/152 on the official suite at 2026-07-28) and ACP support.

- **Engine:** decide by measurement whether rmcp replaces the in-house engine; this follows the owner's rule to prefer the best upstream crates.
- **Exposure:** the gateway lives in fw, is exposed through `fw acp` / `fw mcp`, and replaces the per-service MCP servers configured for Claude Code where a direct UTCP call exists.

## Proof

- **Real tools:** GitHub through its official MCP server, and the Cloudflare API through OpenAPI with UTCP direct calls.
- **Per call:** latency and context bytes, against the current setup.
- **Conformance:** the official MCP suite still passes.
