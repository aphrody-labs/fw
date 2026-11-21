# Microsoft Intelligent Terminal (`wta_core`) & Rio (`rio-ai`) Architecture

Verified live on `2026-11-19` per `/aphrody:microsoft` truth protocol against `github.com/microsoft/intelligent-terminal` and the fw-nt checkout (`third_party/intelligent-terminal/tools/wta`).

## 1. Live Upstream Provenance

- **Upstream**: `microsoft/intelligent-terminal` (`main`, `pushed_at: 2026-11-19T16:36:09Z`, `2,247` stars, MIT)
- **Latest verified commits**:
  - `0d4f852f` (`2026-11-19T02:57:49Z`) — `docs: update README sidebar and agent configuration (#1423)`
  - `5d9a8ad9` (`2026-11-18T07:19:45Z`) — `Document OSC 7501 sidebar status for users and script authors (#1422)`
  - `4f4c8345` (`2026-11-17T13:27:44Z`) — `Add OSC 7501 program status (#1388)`
- **Aphrody Labs Forks (synced `2026-11-19`)**:
  - `aphrody-labs/intelligent-terminal`
  - `aphrody-labs/rio` (fork of `raphamorim/rio`, `pushed_at: 2026-11-19T18:24:14Z`)

## 2. Core Library Extraction & Purge in `third_party/intelligent-terminal/tools/wta` (fw-nt)

- Purged `618 MiB` of stale `third_party/intelligent-terminal/tools/wta/target/` build cache and dead `src/test_support.rs` + dangling `#[cfg(test)]` declarations in `src/main.rs`.
- Exposed reusable library target `[lib] name = "wta_core"` (`third_party/intelligent-terminal/tools/wta/src/lib.rs`) exporting:
  - `osc52` — OSC 52 base64 clipboard escape sequences
  - `rio_bridge` (`third_party/intelligent-terminal/tools/wta/src/rio_bridge.rs`) — OSC 7501 program status codec, built-in ACP agent registry (`copilot`, `claude`, `codex`, `gemini`, `opencode` and others), Session MCP tool catalog (`run_command_in_current_shell`, `create_workspace`, `delegate_task_in_new_workspace`, `request_user_input`), Native Windows ConPTY + NamedPipe SDDL config, and Cloudflare Workers AI / AI Gateway edge config.

## 3. Rio GPU Terminal Update (`aphrody-labs/rio` — `rio-ai`)

Added `#![forbid(unsafe_code)]` workspace crate `rio-ai` (`$WORKSPACE_ROOT/rio/rio-ai`) providing:
- **Full AI features**: Per-tab pre-warmed agent pane stash/restore without wiping ACP session state, OSC 7501 status updates, OSC 133 command-failure autofix queueing, and Session MCP JSON-RPC 2.0 dispatcher.
- **Native Windows High Support**: In-process ConPTY, per-window NamedPipe (`\\.\pipe\rio-ai-master-<id>`) with current-user SDDL (`D:(A;;GA;;;OW)`), `wgpu-dx12` + DirectWrite, Win32 clipboard DIB->PNG, and Job Object kill-on-close.
- **Cloudflare High Support**: Workers AI + AI Gateway (`gateway.ai.cloudflare.com/v1`), Streamable HTTP `/mcp`, `/acp`, `/a2a`, Zero Trust Access + `CF-Connecting-IP` restoration, and `librio-wasm` Workers Static Assets edge profile.
