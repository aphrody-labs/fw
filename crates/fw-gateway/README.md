# fw-gateway

One in-process tool gateway for agents.

- UTCP (`rs-utcp` 0.3.2) holds the tool repository and calls APIs directly. Search is its `TagSearchStrategy`; the words of each tool name are added as tags, because the strategy scores tags and descriptions only. On ten intent queries over Cloudflare and GitHub it beat a word-rarity ranking written for the purpose, which was dropped. An OpenAPI document becomes one UTCP tool per operation, called over HTTPS with no server in between.
- Existing MCP servers are consumed through the rmcp 3.11 client (`server/discover` first, `initialize` as the fallback) and registered as a UTCP protocol.
- The catalogue is served to agents as MCP: `fw-gateway serve` over stdio, `fw-gateway acp` as an ACP proxy that attaches it as an MCP-over-ACP server (`McpServer::from_rmcp`, no process, no port).
- Three meta-tools keep the context cost flat: `search_tools`, `call_tool`, `run_codemode` (Rhai). `expose = "typed"` or `"both"` also lists every tool first-level.
- Secrets are environment variable names in the config, read on the host at call time. A third-party manual can only use `http` to an `https` URL; the `cli` and `text` protocols of rs-utcp are replaced by refusals.
- A business error is `Ok(CallToolResult::error)` (`isError: true`); a JSON-RPC error is only for a malformed call to a meta-tool.

```
fw-gateway --config gateway.toml serve
fw-gateway --config gateway.toml tools "list dns records"
fw-gateway --config gateway.toml info
```

`gateway.example.toml` shows the configuration. The crate has its own `[workspace]` and lockfile until it joins the fw workspace. Provider names are process-wide (the rs-utcp protocol registry is global), so one process runs one gateway.

`rs-utcp` 0.3.2 inlines `$ref`s without a limit and overflows the stack on recursive schemas (the Cloudflare document has them), so `src/http.rs` resolves references under a depth limit and drops response schemas before its converter runs; it also calls one URL per provider, so the gateway keeps one route per operation. It does not compile against rhai 1.27 unless `smartstring` has its `serde` feature, hence the direct dependency in `Cargo.toml`.
