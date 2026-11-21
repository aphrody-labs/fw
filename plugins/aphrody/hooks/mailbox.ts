#!/usr/bin/env bun
// UserPromptSubmit hook: hands the session the messages queued for it by scripts/notify.ts or claude_inject when the
// messaging pipe could not carry them (see configs/claude/scripts/lib/claude-peer.ts).
import { resolve } from "node:path";

const { drainMail } = (await import(
  resolve(import.meta.dir, "..", "..", "..", "configs", "claude", "scripts", "lib", "claude-peer.ts")
)) as typeof import("../../../configs/claude/scripts/lib/claude-peer.ts");

const input = (await new Response(Bun.stdin.stream()).text().then(
  (text) => (text.trim() ? JSON.parse(text) : {}),
  () => ({}),
)) as { session_id?: string; hook_event_name?: string };

const mail = input.session_id ? drainMail(input.session_id) : [];
if (mail.length > 0) {
  process.stdout.write(
    JSON.stringify({
      hookSpecificOutput: {
        hookEventName: input.hook_event_name ?? "UserPromptSubmit",
        additionalContext: mail.map((item) => `[Live peer message from ${item.from}]: ${item.text}`).join("\n"),
      },
    }),
  );
}
