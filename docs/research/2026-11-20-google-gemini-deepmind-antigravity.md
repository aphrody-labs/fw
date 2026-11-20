# Research: Google, Gemini, DeepMind & Antigravity Audit (2026-11-20)

Live snapshot fetched via `gh api` and `agy --version` on `2026-11-20`.

## 1. Live Ecosystem Versions
- **Antigravity CLI (`agy`)**: `1.5.13` installed locally (`%LOCALAPPDATA%\agy\bin\agy.exe`); strict plugin validator enforces `mcp_config.json` (not `.mcp.json`) and rejects unknown fields like `"type"` on HTTP MCP entries.
- **`google-gemini/gemini-cli`** (`v0.70.0`, `108,950` stars, TypeScript): active core fixes including nested `.gitignore` negations (`d4cb1713fd`).
- **`googleapis/python-genai`** (`v3.6.1`, `4,081` stars, Python) & **`googleapis/js-genai`** (`v3.6.1`, `1,715` stars, TypeScript): unified Gemini GenAI SDKs (SSE UTF-8 streaming, `SpeechConfig.text_normalization`, Interactions/Agents).
- **`google/adk-python`** (`v2.15.0`, `22,282` stars, Python): Agent Development Kit with in-place workflow child retries (`c81876a499`).
- **`a2aproject/A2A`** (`v1.0.1`, `26,607` stars): canonical Agent-to-Agent protocol specification.
- **`google-deepmind`**:
  - `robotics-antigravity-plugin` (`165` stars, pushed `2026-11-20`): official DeepMind Antigravity plugin reference.
  - `gemma` (`5,862` stars, pushed `2026-11-19`), `science-skills` (`3,506` stars), `mujoco_warp` (`1,646` stars).

## 2. Improvements Applied to `fw`
1. **Cron Sync Expansion**: Added `google-gemini/gemini-cli`, `googleapis/python-genai`, `googleapis/js-genai`, `google/adk-python`, `google-deepmind/robotics-antigravity-plugin`, `google-deepmind/gemma`, and `a2aproject/A2A` to `.github/workflows/upstream-research-sync.yml`.
2. **Antigravity Spec Refresh**: Updated `plugin/skills/agy-knowledge/SKILL.md` to reflect `agy 1.5.13` and `mcp_config.json` strict validation rules (`0` warnings on `fw` plugin).
3. **Next Targets in `fw`**:
   - Align `crates/fw-a2a` with `a2aproject/A2A` `v1.0.1` schema.
   - Align `crates/fw-gemini` (`boq`, `web`, `runtime`, `google`) with `genai` `v3.6.1` SSE and speech/multimodal contracts.
   - Inspect `google-deepmind/robotics-antigravity-plugin` and `google-deepmind/science-skills` for `fw` plugin packaging patterns.
