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
- `gemini-decision-1` is not in the Vertex catalog (`GET .../models/gemini-decision-1` answers 404). It is a Gemini API (AI Studio) model: the live pages https://aistudio.google.com/docs/models/gemini-decision-1.md and https://aistudio.google.com/docs/decisions-api.md answer 200 and document `POST https://generativelanguage.googleapis.com/v1beta/decisions` with `"model": "gemini-decision-1"`. `gemini-4-neon` and `gemini-decision-1` therefore both exist; no code or config naming them was deleted.

### Gemini API model list (live, 2026-11-21)

Credential path: every OAuth token (user and service account) is refused by `generativelanguage.googleapis.com`: service accounts answer 403 "Access to Gemini API is restricted with service accounts", and user tokens with the `cloud-platform` scope answer 403 "insufficient authentication scopes" (the `generative-language` scope cannot be added to the gcloud session without a browser consent). The working path is the existing "Default Gemini API Key" of the AI Studio project (restricted to `generativelanguage.googleapis.com`), read with `gcloud services api-keys get-key-string <uid> --project <ai-studio project>` under the primary Google account and sent as the `x-goog-api-key` header from a header file (never argv or logs). `GET /v1beta/models?pageSize=1000` answered 200 with 65 models and no `nextPageToken`.

| Model | Methods | Input / output tokens |
|---|---|---|
| `gemini-4-argon`, `gemini-4-carbon` | generateContent, countTokens, batchGenerateContent | 1,048,576 / 32,768 |
| `gemini-4-hydrogen`, `gemini-4-neon` | generateContent, countTokens, batchGenerateContent | 1,048,576 / 65,536 |
| `gemini-3.8-flash`, `gemini-3.7-flash`, `gemini-3.6-flash`, `gemini-3.5-flash` | generateContent, countTokens, createCachedContent, batchGenerateContent | 1,048,576 / 65,536 |
| `gemini-omni-1.2-flash`, `gemini-omni-1.1-flash`, `gemini-omni-flash-preview` | generateContent, countTokens | 131,072 / 65,536 |
| `gemini-decision-1` | createDecision | 262,144 / 1 |
| `antigravity-preview-09-2026`, `antigravity-preview-05-2026`, `antigravity-preview-latest` | generateContent, countTokens | 1,048,576 / 65,536 |
| `deep-research-max-preview-04-2026`, `deep-research-preview-04-2026` | generateContent, countTokens | 1,048,576 / 65,536 |
| `gemini-3.8-live`, `gemini-3.8-live-extended-thinking`, `gemini-3.1-flash-live-preview`, `genie-4` | bidiGenerateContent | 131,072 / 65,536 (`genie-4`: 1,048,576 / 65,536) |
| `gemini-embedding-2`, `gemini-embedding-001` | embedContent, asyncBatchEmbedContent | 8,192 and 2,048 input / 1 |
| `lyria-3.5`, `lyria-3-pro-preview`, `lyria-realtime-exp` | generateContent / bidiGenerateMusic | 1,048,576 / 65,536 |

The remaining entries are older Gemini 2.5 and 3.x text, image (`gemini-3.1-flash-image`, `gemini-3-pro-image`, `nano-banana-pro-preview`), TTS, robotics, computer-use and Gemma 4 (`gemma-4-26b-a4b-it`, `gemma-4-31b-it`) models. `gemini-decision-1` and the Antigravity agent models exist on the Gemini API only (absent from the Vertex catalog above).

## Not kept

The Antigravity CLI operating spec (flags, hooks, plugin and sidecar layout, `~/.gemini` paths), the `localharness` SDK notes and the Gemini 4 pricing figures (the model list carries token limits only, no prices). They described a tool that is retired.
