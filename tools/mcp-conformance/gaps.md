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
| Streamable HTTP (2026-07-28): single POST endpoint, JSON or request-scoped SSE, `202` for notifications, `405` for GET and DELETE, session headers ignored, `X-Accel-Buffering`, keep-alive comments | done | `mcp_protocol.rs`, `server-sse-multiple-streams` |
| Request headers `Mcp-Method`, `Mcp-Name`, `Mcp-Param-*` (`x-mcp-header`) validated with base64 decoding and numeric comparison | done | `http-header-validation`, `http-custom-header-server-validation` |
| Origin and Host validation against DNS rebinding | done | `dns-rebinding-protection`; loopback binds accept loopback names only, other binds read `FW_MCP_ALLOWED_HOSTS` |
| stdio: newline-delimited messages, `notifications/cancelled`, exit on end of input, nothing but messages on stdout | done | `mcp_protocol.rs` (cancel stops work and sends nothing for the request) |
| Progress notifications only for requests with a `progressToken`; log messages only with `logLevel` | done | `mcp_protocol.rs`, `tools-call-with-progress` |
| Pagination with opaque cursors, `-32602` for an invalid cursor; deterministic tool order | done | `mcp_protocol.rs`, `tools.test.ts` |
| Tool errors as `isError` results (invalid arguments are validated against the `inputSchema`); unknown tool is `-32602` | done | `tools.test.ts`: 21 tools, valid and invalid payload, both transports |
| JSON Schema 2020-12 `inputSchema` passed through unchanged; `$ref` to a network URI never followed | done | `json-schema-2020-12` (8 checks). The argument validator covers type, enum, const, required, properties, additionalProperties, items, min/max, allOf/anyOf/oneOf/if; `pattern` and `format` are not checked |
| Resources, prompts, completions, logging: engine support | done for the engine, absent in fw's catalog | fw exposes tools only; the capability is not declared and the methods answer `-32601`. The fixture serves all four |
| Earlier versions (2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05): `initialize`, `ping`, `tools/*`, resources, prompts, logging, completion | done | suite runs at 2025-11-25 and 2025-06-18 |
| Earlier versions: server-initiated sampling and elicitation requests on the response stream of a call (`sampling/createMessage`, `elicitation/create`) | done | suite `tools-call-sampling`, `tools-call-elicitation`, `elicitation-sep1034-defaults`, `elicitation-sep1330-enums` at 2025-11-25 and 2025-06-18; `mcp_protocol.rs` over HTTP sessions and stdio. A client that did not declare the capability is not asked; the answer wait ends after 120 s, on cancellation or when the connection closes. Removed by 2026-07-28 (multi round-trip requests replace them) |
| Earlier versions: `Mcp-Session-Id` sessions (issued at `initialize`, `404` once unknown, ended by `DELETE`), priming event with `id` and `retry`, resumption of a closed response stream with `GET` and `Last-Event-ID` | done | suite `server-sse-polling` (3 checks incl. priming, retry and disconnect-resume) and `server-sse-multiple-streams` at 2025-11-25; a request outside a session is served statelessly, `GET` without `Last-Event-ID` answers `405`, and 2026-07-28 requests ignore the session headers |
| Earlier transport HTTP+SSE (2024-11-05: `GET /sse` plus a message endpoint) | absent | superseded by Streamable HTTP in 2025-03-26; no client in the suite speaks it. Owner: nobody needs it |
| Extension `io.modelcontextprotocol/tasks` | absent | not advertised under `capabilities.extensions`; no fw tool is long-running. Owner: queue lot `fw-mcp-tasks`, not in v2026.11.0 |
| Authorization (OAuth 2.1 resource server) for HTTP | absent | loopback by default; non-loopback binds need a reverse proxy. Owner: queue lot `fw-mcp-auth`, not in v2026.11.0 |
| `crates/fw-edge` Worker `/mcp` (separate Rust and TypeScript implementations, answers `initialize`, `tools/list`, `tools/call`, `ping`) | absent | not covered here. Owner: queue lot `fw-edge-mcp-conformance`, not in v2026.11.0 |
| Plugin fallback `src/mcp_server.ts` (Bun) | partial | both eras and `server/discover`, tested in `tests/fw_plugin_daemon.test.ts`; the official suite is not run against it because its tools are not the fixture catalog. The plugin launches `fw mcp` whenever a binary that answers `server/discover` exists |
