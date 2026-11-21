# Cross-Provider AI Triad Architecture & Protocol Interconnect

## 1. Executive Topology

`@aphrody/ai-triad` (`packages/ai-os/engines/ai-triad`) unifies **OpenAI Codex (`codex`)**, **Google Gemini (`gemini`)**, and **Anthropic Claude Code (`claude`)** alongside **OpenCode**, **Pi**, **Zed**, and local **Sovereign AI-OS / Ring-3 CUDA (`sm_89`)** daemons using strictly documented, standard protocols:

- **MCP `2025-03-26` (Stdio & Streamable HTTP)**: Bidirectional cross-agent tool invocation (`triad mcp-stdio` and `POST http://127.0.0.1:4319/mcp`).
- **ACP v1 (Agent Client Protocol)**: Native JSON-RPC stdio session orchestration across `gemini --acp` and Zed external agents.
- **OpenAI Chat Completions (`POST /v1/chat/completions`)**: Universal wire interface consumed by Zed `openai_compatible`, OpenCode `providers`, Pi, Continue, and CI jobs.
- **OpenAI Responses API (`POST /v1/responses`)**: Native wire protocol required by Rust `codex-rs` custom `[model_providers.*]` (`wire_api = "responses"`).
- **Anthropic Messages API (`POST /v1/messages`)**: Native wire format consumed by Claude Code and CC-Switch provider profiles.
- **OAuth 2.1 + PKCE (`S256`) + RFC 8628 Device Authorization Grant**: Standards-compliant public-client login flows and ACL-locked AES-GCM token metadata vaulting without ever touching private auth gates or spoofing client checks.

```mermaid
flowchart LR
  subgraph TUIs_IDEs["Interactive TUIs & IDEs"]
    ClaudeTUI["Claude Code v2.1.318"]
    CodexTUI["Codex CLI v0.162.0 (Rust)"]
    GeminiTUI["Gemini CLI v0.66.0"]
    ZedIDE["Zed Editor + OpenCode + Pi"]
  end

  subgraph Hub["@aphrody/ai-triad (:4319 + triad.exe)"]
    MCP["MCP 2025-03-26 Server (stdio + /mcp)"]
    OAI["/v1/chat/completions + /v1/responses"]
    ANT["/v1/messages"]
    Router["Quota-Aware Router & Parallel Council"]
    OAuth["OAuth 2.1 PKCE S256 + RFC 8628 Broker"]
  end

  subgraph Backends["Headless Runners & Local Inference"]
    ClaudeP["claude -p --output-format json"]
    CodexE["codex exec --json / review"]
    GeminiP["gemini -p -o json / gemini --acp"]
    Local["AI-OS (:4318) + Rust Gateway (:4320) + Ring-3 CUDA (:8787)"]
  end

  ClaudeTUI <-->|"MCP stdio"| MCP
  CodexTUI <-->|"MCP stdio + /v1/responses"| Hub
  GeminiTUI <-->|"MCP stdio + ACP"| Hub
  ZedIDE <-->|"MCP + /v1/chat/completions"| Hub

  MCP --> Router
  OAI --> Router
  ANT --> Router
  Router --> ClaudeP
  Router --> CodexE
  Router --> GeminiP
  Router --> Local
```

## 2. Bidirectional Cross-CLI Matrix

| Host Environment | Inbound Config File | How It Calls Peer Agents | Outbound Headless Invocation |
| :--- | :--- | :--- | :--- |
| **Claude Code (`claude`)** | `~/.claude.json`, `~/.claude/mcp_sovereign.json`, `<workspace>/CLAUDE.md` | Calls `triad_ask_codex`, `triad_ask_gemini`, `triad_council_review` via `ai-triad` MCP | `claude -p "<prompt>" --output-format json --mcp-config ...` |
| **OpenAI Codex (`codex`)** | `~/.codex/config.toml`, `~/.codex/local.config.toml`, `~/.codex/triad.config.toml`, `~/.codex/AGENTS.md` | Calls `triad_ask_claude`, `triad_ask_gemini`, `triad_council_review` via `ai_triad` MCP or `-p triad` (`/v1/responses`) | `codex exec "<prompt>" --json --skip-git-repo-check` or `codex exec review` |
| **Google Gemini CLI (`gemini`)** | `~/.gemini/settings.json`, `~/.gemini/projects.json`, `<workspace>/GEMINI.md` | Calls `triad_ask_claude`, `triad_ask_codex`, `triad_council_review` via `ai-triad` MCP; ACP (`gemini --acp`) | `gemini -p "<prompt>" -o json` |
| **OpenCode & Pi** | `~/.config/opencode/opencode.jsonc` | `providers.ai_triad` (`:4319/v1`) + `mcp.ai_triad` | `pi-opencode` + Ring-3 local fallback (`:8787/v1`) |
| **Zed Editor** | `%APPDATA%\Zed\settings.json` | `language_models.openai_compatible.ai_triad` + `context_servers.ai-triad` | Consumes `triad/auto` and `triad/council` models |

## 3. Reference Ecosystem Integration (`<workspace>/third_party/ai-ecosystem`)

1. **`openai/codex`**: Rust workspace (`codex-rs`) implementing `WireApi::Responses` (`/v1/responses`), Profile V2 (`$CODEX_HOME/<name>.config.toml`), MCP client/server, and Device Code / OAuthkeyring auth.
2. **`anthropics/claude-code`**: Reference plugins, hooks, `CLAUDE.md` conventions, and headless `--output-format stream-json|json` automation patterns.
3. **`google-gemini/gemini-cli`**: TypeScript workspace implementing `--acp`, MCP, skills, hooks, and strict Zod-validated `projects.json` slug registry.
4. **`farion1231/cc-switch` & `SaladDay/cc-switch-cli`**: Cross-CLI profile switchers managing Claude Code, Codex, Gemini CLI, OpenCode, and Pi provider endpoints and MCP registries.
5. **`cheaper-inference/OmniRoute`**: Quota-aware multi-provider routing gateway inspiring `routeTask()` HTTP 429 / quota failover chains (`gemini -> claude -> codex -> rust-gateway -> ring3-local`).
6. **`lidge-jun/opencodex` & `router-for-me/CLIProxyAPI`**: Multi-protocol proxy patterns translating cleanly between OpenAI Chat Completions, OpenAI Responses, and Anthropic Messages schemas.

## 4. Security & Zero-Spoofing Guarantees

- **Zero Raw Secret Exposure**: `inspectCrossAuthStatus()` inspects file existence, byte sizes, top-level schema keys, and masked environment lengths (`SET [redacted, len=...]`) without ever logging or returning token strings.
- **Official Flows Only**: Uses official CLI headless flags (`claude -p`, `codex exec`, `gemini -p`), official ACP/MCP JSON-RPC transports, and RFC 7636 PKCE `S256` + RFC 8628 Device Code state machines.
