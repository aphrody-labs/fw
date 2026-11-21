# fw-mcp-followups: what the 2026-07-28 conformance work left out (queued)

Phase 2, after v2026.11.0. Opened by the lot `fw-mcp-conformance`; the table in `docs/research/mcp-conformance.md` names each row.

## Lots
- `fw-mcp-auth`: OAuth 2.1 resource-server behaviour for `fw serve` on a non-loopback bind (protected resource metadata, bearer validation, `WWW-Authenticate`). Spec: `basic/authorization/`.
- `fw-mcp-tasks`: the `io.modelcontextprotocol/tasks` extension, only if a fw tool becomes long-running (`tasks/get`, `tasks/update`).
- `fw-edge-mcp-conformance`: run the pinned official suite against `crates/fw-edge` (Rust and TypeScript Workers) and either port the engine in `crates/fw-proto/src/mcp.rs` to the edge or record the divergence.

## Acceptance
Each lot adds its scenarios to `tools/mcp-conformance/suite.json` and the gaps table row moves from absent to done.
