---
name: claude-live-bridge
description: "Push real-time context and action updates to live Claude Code sessions through their native messaging pipe (\\\\.\\pipe\\LOCAL\\cc-msg-*) with a mailbox fallback"
---

# Real-time context injection into live Claude Code sessions

Use this bridge to tell other running sessions what you are doing, to split disjoint file scopes, or to wake an idle session without waiting for it to restart.

## 1. MCP server `claude-bridge` (plugin `aphrody`)

- `claude_sessions`: live sessions with `pid`, `sessionId`, `name`, `cwd`, `status` and `pipeReady`.
- `claude_inject { message, target?, sender?, cwd?, urgent? }`:
  - Writes to the session's authenticated messaging pipe: an `auth` NDJSON frame with the peer token from `~/.claude/sessions/<pid>.<hash>.key`, then a `user` frame wrapped in `<cross-session-message from-name="...">`. Mid-turn or idle, the pipe delivers at once.
  - Claude Code accepts a pipe message only from a sender it knows as a live session. When the sender is not one (a script, CI), or the pipe write fails, the message goes to the mailbox `~/.claude/mailbox/<sessionId>/` and the plugin hook `UserPromptSubmit` (`hooks/mailbox.ts`) hands it to the session on its next prompt.
  - A broadcast (`target: "all"`) skips the calling session.

## 2. CLI `notify.ts`

```sh
bun ~/.claude/scripts/notify.ts --list
bun ~/.claude/scripts/notify.ts --to all "Config updated, re-read settings."
bun ~/.claude/scripts/notify.ts --to <sessionId|name|pid|cwd-basename> --urgent "Pause edits on the shared file."
```

`--from <name>` sets the sender (default: the calling session's name, else `notify`); `--cwd <path>` limits the targets; `--mailbox` also queues the mailbox copy. The result is a JSON array of `{target, sessionId, channel: "pipe"|"mailbox", ok}`; the pipe sends no acknowledgement, so `ok` means the frames were written.

The implementation is `configs/claude/scripts/lib/claude-peer.ts` of the fw repository; `configs/claude/scripts/deploy.ts` copies it to `~/.claude/scripts`.
