## Specification 2026-07-28 against fw: gap table

Read live from `docs/specification/2026-07-28/` (changelog, basic, transports, server) and `schema/2026-07-28/`. Status: done = implemented and covered by the suite or `crates/fw-proto/tests/mcp_protocol.rs`; absent = not implemented, with the owner of the decision.

| Spec item | Status | Evidence or owner |
|---|---|---|
| Stateless requests: version, client capabilities and identity in `_meta`; missing fields are `-32602` and HTTP 400 | done | suite `server-stateless` (30 checks) |
| `server/discover` (versions, capabilities, instructions, serverInfo in `_meta`, cache hints) | done | `server-stateless`, `caching` |
| `UnsupportedProtocolVersionError` (`-32022`), `MissingRequiredClientCapabilityError` (`-32021`), `HeaderMismatch` (`-32020`), error code policy | done | `server-stateless`, `http-header-validation` |
| `resultType` on every result; `ttlMs` and `cacheScope` on the six cacheable methods | done | `caching`, wire-schema check on every message |
| Removed: `initialize`, `ping`, `logging/setLevel`, `resources/subscribe` answer `404` and `-32601` in the 2026-07-28 era | done | `server-stateless` |
| `subscriptions/listen` (acknowledgement first, subscription id, filtering, cancellation by closing the stream) | done | `server-stateless`; unit tests over stdio and HTTP. fw's own catalog never changes, so it declares `listChanged: false` and a listen acknowledges an empty filter |
| Multi round-trip requests (`input_required`, `inputResponses`, `requestState` integrity) | done | 14 `input-required-result-*` scenarios; fw's own tools never need input |
| Streamable HTTP: single POST endpoint, JSON or request-scoped SSE, `202` for notifications, `405` for GET and DELETE, session headers ignored, `X-Accel-Buffering`, keep-alive comments | done | `mcp_protocol.rs`, `server-sse-multiple-streams` |
| Request headers `Mcp-Method`, `Mcp-Name`, `Mcp-Param-*` (`x-mcp-header`) validated with base64 decoding and numeric comparison | done | `http-header-validation`, `http-custom-header-server-validation` |
| Origin and Host validation against DNS rebinding | done | `dns-rebinding-protection`; loopback binds accept loopback names only, other binds read `FW_MCP_ALLOWED_HOSTS` |
| stdio: newline-delimited messages, `notifications/cancelled`, exit on end of input, nothing but messages on stdout | done | `mcp_protocol.rs` (cancel stops work and sends nothing for the request) |
| Progress notifications only for requests with a `progressToken`; log messages only with `logLevel` | done | `mcp_protocol.rs`, `tools-call-with-progress` |
| Pagination with opaque cursors, `-32602` for an invalid cursor; deterministic tool order | done | `mcp_protocol.rs`, `tools.test.ts` |
| Tool errors as `isError` results (invalid arguments are validated against the `inputSchema`); unknown tool is `-32602` | done | `tools.test.ts`: 21 tools, valid and invalid payload, both transports |
| JSON Schema 2020-12 `inputSchema` passed through unchanged; `$ref` to a network URI never followed | done | `json-schema-2020-12` (8 checks). The argument validator covers type, enum, const, required, properties, additionalProperties, items, min/max, allOf/anyOf/oneOf/if; `pattern` and `format` are not checked |
| Resources, prompts, completions, logging: engine support | done for the engine, absent in fw's catalog | fw exposes tools only; the capability is not declared and the methods answer `-32601`. The fixture serves all four |
| Earlier versions (2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05): `initialize`, `ping`, `tools/*`, resources, prompts, logging, completion | done | suite runs at 2025-11-25 and 2025-06-18 |
| Earlier versions: server-initiated sampling and elicitation requests on an open stream | absent | 2 scenarios in `expected-failures.yml` (`tools-call-sampling`, `tools-call-elicitation`; 2 more for the SEP-1034 and SEP-1330 elicitation variants). Removed by 2026-07-28; owner: nobody needs it, revisit only for a client that cannot speak 2026-07-28 |
| Earlier versions: `Mcp-Session-Id` sessions, GET stream, `Last-Event-ID` resumption, HTTP+SSE transport | absent by design | `server-sse-polling` and `server-sse-multiple-streams` at 2025-11-25 are in the baseline; the spec tells a 2026-07-28 server to answer `405` and ignore the headers |
| Extension `io.modelcontextprotocol/tasks` | absent | not advertised under `capabilities.extensions`; no fw tool is long-running. Owner: queue lot `fw-mcp-tasks`, not in v2026.11.0 |
| Authorization (OAuth 2.1 resource server) for HTTP | absent | loopback by default; non-loopback binds need a reverse proxy. Owner: queue lot `fw-mcp-auth`, not in v2026.11.0 |
| `crates/fw-edge` Worker `/mcp` (separate Rust and TypeScript implementations, answers `initialize`, `tools/list`, `tools/call`, `ping`) | absent | not covered here. Owner: queue lot `fw-edge-mcp-conformance`, not in v2026.11.0 |
| Plugin fallback `src/mcp_server.ts` (Bun) | partial | both eras and `server/discover`, tested in `tests/fw_plugin_daemon.test.ts`; the official suite is not run against it because its tools are not the fixture catalog. The plugin launches `fw mcp` whenever a binary that answers `server/discover` exists |
