---
name: agy-knowledge
description: Verified operational knowledge base for the complete Google Antigravity stack (Desktop App, agy CLI, IDE extensions, ACP JSON-RPC, localharness Python SDK, community Go/TS ports, Managed Agents Cloud API, MCP, rules, skills, hooks, plugins, sidecars, and dynamic workflows) unified inside Fw. Load whenever any LLM needs to operate, automate, debug, or extend Google Antigravity without schema errors or version traps.
---

# agy-knowledge — Verified Antigravity Stack Operating Spec (Fw Unified)

Reference JSON: `references/agy-knowledge.verified.json` and `references/protocols-recon.verified.json`; narrative spec: `references/agy-knlogie.md`.

## 1. Runtime & Surface Selection
- **Antigravity CLI (`agy`)**: Zero-dependency Google-signed native binary (`%LOCALAPPDATA%\agy\bin\agy.exe`). Default mode since **v1.5.7** is **`auto`** (including `-p` headless runs); pin `"agentMode": ""` in `~/.gemini/antigravity-cli/settings.json` to restore `default`. Inside Fw, `target/agy/agy.exe` (Rust Fat-LTO AOT) wraps and accelerates Fw operations while leaving Google's signed binary strictly read-only.
- **Antigravity App**: Standalone GUI supporting multi-agent orchestration, built-in Chromium, Marketplace plugins, and **Sidecars** (App-only; NOT supported in CLI or standalone IDE).
- **Antigravity IDE / Extensions**: Local Language Server on `127.0.0.1` over **ConnectRPC** with ephemeral per-session **CSRF token**; local state in SQLite `state.vscdb` (Electron ABI v140 / Node v22.21.1 breaks native `better-sqlite3` — use WASM `sql.js`).
- **Antigravity Python SDK (`google-antigravity`)**: Only official Google SDK (Apache-2.0; Go and TS on roadmap per Issue #125). Install via `pip install google-antigravity` (`localharness` ships only in PyPI wheels). Default model: `gemini-3.8-flash`.
- **Managed Cloud Agents API (`google-genai` `v1beta`)**: Hosted isolated Linux sandbox via `agent="antigravity-preview-09-2026"` (`1,048,576` input window auto-compacted at `~135k`; `65,536` output limit; text+image input, text output).

## 2. CLI (`agy`), Headless NDJSON & ACP Contracts
- **Single-shot headless**: `agy -p "<prompt>" --output-format text|json|stream-json [--json-schema '<schema>']`
- **Bidirectional streaming**: `agy --input-format stream-json --output-format stream-json` (never pair with `-p`; send `{"event":"user","message":{"content":"..."}}`, wait for `result` before next prompt).
- **ACP**: `agy acp` (JSON-RPC over stdin/stdout; `agy acp --login`).
- **GCP observability**: `GOOGLE_CLOUD_PROJECT`, `AGY_LOG_EXPORTER=gcp`, `AGY_OTEL_EXPORTER=gcp`.
- **Fw LTO/AOT inspection & upgrade**: `fw inspect` (craft-pe Fat-LTO PE32+ report on `agy.exe`), `fw upgrade --with-agy --dry-run`, `target/agy/agy.exe --fw-info`.

## 3. Extensibility Invariants
- **Rules**: `AGENTS.md`/`GEMINI.md` = plain Markdown (no YAML frontmatter). `.agents/rules/*.md` & `~/.gemini/config/rules/*.md` = flat scan, require YAML frontmatter (`trigger: always_on | model_decision | glob | manual`, `description`, `globs`).
- **Hooks**: `PreToolUse`/`PostToolUse` require `matcher` + nested `"hooks": [...]`; `SessionStart`/`PreInvocation`/`PostInvocation`/`Stop` take a **direct handler array** (extra `"hooks"` wrapper crashes Language Server JSON parsing).
- **Plugins (v1.5.7+)**: `.agents/plugins.json` and `.agents/skills.json` skip paths escaping workspace root. Materialize `~/.gemini/config/plugins/<name>` as real directories (no NTFS junctions) and validate via `agy plugin validate <dir> --strict --json`.
- **Sidecars**: App-only (`~/.gemini/config/sidecars/<name>/sidecar.json` + `~/.gemini/config/config.json`). Consumer TOS forbids relaying/wrapping Antigravity inside third-party coding agents.
