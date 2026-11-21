# fw-mcp-conformance: every fw MCP tool tested one by one against the current MCP spec (queued)

Phase 2 (fw CLI and MCP server). Owner: the Claude Desktop session "Audit et déploiement du framework". Release v2026.11.0: blocks the "fw CLI and MCP" artifact, whose gate needs an installed server answering real calls.

## Goal
agy conversation e5ed984f (2026-11-18) admitted that only one tool per MCP server had been called (for example 1 of the aphrody_os-craft tools and 1 of 5 bun-brain tools) and that the server surface was never compared with the MCP spec dated 2026-07-28 (stateless core, routing by headers). The user's demand there: every tool tested one by one, code read one by one, with proof. The servers it covered (relay, bun-brain, os-craft) are gone; the same gap exists for the `fw` MCP server in `crates/fw-bin` and the plugin's servers.

## Do
1. Enumerate the live tool list from the installed fw MCP server (`tools/list`), not from docs.
2. For each tool: one typed Bun test that calls it with a valid and an invalid payload and asserts on the body (not on status), run in CI on the fr1 build; table of tool, test, result in `docs/research/`.
3. Read the spec at modelcontextprotocol.io for 2026-07-28 (live) and list gaps: stateless handling, header routing, task and structured-output behaviour; fix or record each.
4. The release gate job re-runs the suite against the installed binary of the tagged SHA.

## Acceptance
100% of listed tools have a passing test recorded by an independent job; the spec gap table has no unowned row.
