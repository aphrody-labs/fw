# Tool gateway: UTCP, rmcp and MCP over ACP (2026-11-21)

Lot `tool-gateway-mcp-over-acp` (queue file `docs/agents/queue/tool-gateway-mcp-over-acp.md`). Code: `crates/fw-gateway`. Every number below comes from a command run on 2026-11-21 on `prd` (8 cores, shared with other builds) or from the CI run named next to it.

## Crates pinned (read live from crates.io the same day)

`rs-utcp` 0.3.2, `rmcp` 3.11.0, `agent-client-protocol` 5.4.0, `agent-client-protocol-rmcp` 5.4.0 (features `rmcp_3`, `unstable_mcp_over_acp`). The ACP schema crate resolves to `agent-client-protocol-schema` 1.19.0 in the lockfile (1.20.0 is the newest on crates.io). The `schema-v1.34.0` tag of `agentclientprotocol/agent-client-protocol` is the schema document's own version line, not a crate version: the same repository's crate release is `v1.20.0`, published the same day.

## What was built

One process, one gateway (`fw-gateway`): rs-utcp holds the tool repository and search; OpenAPI documents become one UTCP tool per operation, called directly over HTTPS; existing MCP servers are consumed through the rmcp client (`ClientLifecycleMode::Auto`) and registered as the UTCP `mcp` protocol; agents reach everything through three meta-tools (`search_tools`, `call_tool`, `run_codemode` on `CodeModeUtcp`) over MCP stdio (`fw-gateway serve`) or MCP over ACP (`McpServer::from_rmcp`, `fw-gateway acp` as a proxy, and the in-process path used by the test). Business errors return `Ok(CallToolResult::error)`; `Err(McpError)` is only for a malformed meta-tool call.

Security as built and tested: secrets are environment variable names in the config, read at call time, never in a schema, result or error; `http` is refused except on loopback; a manual may only register `http` tools on https URLs (`cli` and `text` are replaced in rs-utcp's global registry by refusals, and `allowed_communication_protocols` is set to the provider's own protocol on every provider).

## Defects found in rs-utcp 0.3.2 (worked around in `crates/fw-gateway/src/http.rs`)

1. `OpenApiConverter` inlines `$ref` without a depth or cycle limit: on the Cloudflare document the process aborted with a stack overflow (`fatal runtime error: stack overflow`). The gateway resolves references under a depth limit of 3, drops response schemas and copies path-level parameters into operations before conversion.
2. `UtcpClient::call_tool` calls the provider a manual was registered with, so one HTTP provider is one URL and method. The gateway keeps one route per tool (method, URL template, query, header and body fields), encodes path parameters, and takes the query/path split from the document itself.
3. The crate does not compile against rhai 1.27 (`SmartString: Serialize` not satisfied) unless `smartstring` has its `serde` feature; `fw-gateway` depends on it directly.
4. `UtcpClient::new` prints `Loaded provider` lines with `println!` when a providers file is configured, which would corrupt a stdio protocol; the gateway never sets one.

## Engine decision: rmcp against the in-house engine, by measurement

The official suite (`@modelcontextprotocol/conformance` 0.2.0-alpha.12, the version pinned in `tools/mcp-conformance/suite.json`) against two servers:

| Server | 2026-07-28 | 2025-11-25 | 2025-06-18 |
|---|---|---|---|
| in-house `crates/fw-proto/src/mcp.rs`, CI run 44889389057 (main at 7561e4162a) | 152 passed, 0 failed | 83 passed, 0 failed, 6 scenarios in `expected-failures.yml` | 56 passed, 0 failed, same baseline |
| rmcp 3.11.0 `conformance-server` (tag `rmcp-v3.11.0` of `modelcontextprotocol/rust-sdk`), no baseline file | 153 passed, 0 failed (40 scenarios) | 83 passed, 0 failed (33 scenarios) | 56 passed, 0 failed |

rmcp passes every scenario on all three versions without a baseline, including the six the in-house engine lists as expected failures (sampling, elicitation, SSE polling and multiple streams). Decision: new MCP surfaces use rmcp (done here); replacing the engine in `fw-proto` is a separate lot (see below) gated by the same suite and `tools/mcp-conformance/tools.test.ts`. The in-house engine and its suite are untouched by this lot: the suite stays green (the three runs above are the CI run on main).

## Proof on real tools (release build, `prd`)

Catalogue: Cloudflare OpenAPI document (`cloudflare/api-schemas`, 29,706,255 bytes) and the official GitHub MCP server (`https://api.githubcopilot.com/mcp/`) behind one gateway. Cloudflare: 3,925 UTCP tools; GitHub: 46 tools through the rmcp client; cold start with both providers about 6.3 s wall time, dominated by fetching and converting the 30 MB document.

Context bytes of `tools/list` (what an agent holds before any call):

| Exposure | Tools | Bytes |
|---|---|---|
| gateway meta-tools | 3 | 1,134 (1,178 on the wire) |
| GitHub MCP server, listed directly through rmcp | 46 | 201,007 |
| GitHub through the gateway, `expose = "typed"` (UTCP schema keeps type, properties, required) | 46 | 49,917 |
| Cloudflare through the gateway, `expose = "typed"` | 3,925 | 7,454,230 |

A complete GitHub task through the gateway over stdio ("get the authenticated user"): `initialize` 288 bytes, `tools/list` 1,178, `search_tools` answer 5,409 (8 candidates with input shapes), `call_tool` 601; 7,476 bytes in all, against 201,007 for the direct listing plus a 3,333 byte result envelope (content and structured content both) = 204,340.

Latency per call, same host, same token, interleaved runs of 8 (p50 / p95, ms):

| Call | Through the gateway | Direct |
|---|---|---|
| GitHub `get_me`, round 1 | 353.9 / 380.4 | 359.3 / 417.9 (plain rmcp client) |
| GitHub `get_me`, round 2 | 363.6 / 402.1 | 355.2 / 362.1 |
| Cloudflare account token verify, 20 runs | 218.7 / 261.4 (UTCP direct HTTP) | 220.7 / 284.6 (plain HTTPS client) |

The gateway adds no measurable latency over the network round trip. The direct GitHub baseline is an rmcp client, not the Claude Code process, so it measures the protocol path, not the editor's own overhead.

Search quality, ten intent queries over the same 3,971 tools, rank of the first acceptable tool (`TagSearchStrategy` with the words of each tool name added as tags, against a word-rarity ranking written for the purpose): tag search 1, 2, 1, 4, 1, 1, 1, 1, 2, 3; ranking 3, 1, 1, 3, 1, 1, 1, 4, miss, 6. The upstream strategy won and the custom ranking was deleted.

## Verified end to end by tests (`crates/fw-gateway/tests/gateway.rs`, 5 tests, plus 1 unit test; clippy clean)

OpenAPI tools called directly with path encoding (`a b/c` becomes `a%20b%2Fc`), query and body routing, the secret sent as a header and absent from every result and error; a missing secret names only the variable; manuals cannot reach `cli`, `text` or plain `http`; `fw-gateway serve` over stdio negotiates through rmcp `Auto`, lists meta and typed tools, runs `run_codemode` chaining two calls, returns `isError: true` for a backend 500 and a JSON-RPC error for a malformed `call_tool`; a second gateway consumes that server as an MCP provider; an ACP agent receives `McpServer::Acp` in `session/new` and reaches `tools/list`, `search_tools` and `call_tool` through `mcp/message` without a process or a port.
