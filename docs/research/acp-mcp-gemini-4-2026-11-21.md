# ACP, MCP and Gemini 4: facts that survived a live check (2026-11-21)

Source: the retired `agy-knowledge` skill (verified 2026-11-19). Every line below was re-read from a live API on 2026-11-21; claims that could not be re-read, and everything specific to the retired Antigravity CLI, are dropped.

## Agent Client Protocol (ACP)

- Repository `agentclientprotocol/agent-client-protocol`: not archived, latest release `schema-v1.34.0` (2026-11-20, `gh api .../releases/latest`); the schema directory holds `v1` and `v2`.
- Clients: crate `agent-client-protocol` 5.4.0 (crates.io, updated 2026-11-18), npm `@agentclientprotocol/sdk` 2.8.0.
- Transport is JSON-RPC 2.0 over stdio; an editor starts the agent as a subprocess (Zed `agent_servers` entry with `command` and `args`).

## Model Context Protocol (MCP)

- Specification releases published in `modelcontextprotocol/modelcontextprotocol` (`docs/specification`): `2024-11-05`, `2025-03-26`, `2025-06-18`, `2025-11-25`, `2026-07-28`, plus `draft`. The latest release tag is `2026-07-28` (published 2026-07-28).
- Rust SDK `rmcp` 3.11.0 (updated 2026-11-17); npm `@modelcontextprotocol/sdk` 1.32.2.
- The stdio servers of this repository (`configs/claude/scripts/claude-bridge.ts`) negotiate `2025-11-25` and the three older versions; the Rust fw MCP server tracks `2026-07-28` under the lot `fw-mcp-conformance`.

## Gemini 4 models

Read from the Vertex AI publisher catalog (`GET https://aiplatform.googleapis.com/v1beta1/publishers/google/models`, 36 models) on 2026-11-21:

- Present: `gemini-4-argon`, `gemini-4-carbon`, `gemini-4-hydrogen`, `gemini-4-neon` (`GET .../models/gemini-4-neon` answers 200, launch stage GA), and also `gemini-3.8-flash`, `gemini-3.7-flash`, `gemini-omni-1.2-flash`.
- Absent: `gemini-decision-1` (`GET .../models/gemini-decision-1` answers 404, not in the list). The Gemini API route `GET https://generativelanguage.googleapis.com/v1beta/decisions` answers 404.
- The Gemini API (`generativelanguage.googleapis.com`) model list could not be read: the available credentials are service-account and user OAuth tokens, which that API rejects with `ACCESS_TOKEN_SCOPE_INSUFFICIENT`, and no `GEMINI_API_KEY` exists in the environment. Re-run it with a key when one is provisioned.

## Not kept

The Antigravity CLI operating spec (flags, hooks, plugin and sidecar layout, `~/.gemini` paths), the `localharness` SDK notes, the managed-agent model `antigravity-preview-09-2026` (absent from the Vertex catalog, not readable on the Gemini API) and the Gemini 4 pricing and context-window figures (no live source was readable). They described a tool that is retired.
