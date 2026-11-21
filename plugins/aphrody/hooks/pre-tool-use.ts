#!/usr/bin/env bun
import {
  isBunCheckout,
  bunCapabilities,
  preferHint,
  readHookInput,
  writeJson,
} from "./common.ts";
import { rewriteCommand } from "./rewrite.ts";

const input = await readHookInput();
const toolInput = (input.tool_input ?? {}) as Record<string, unknown>;
const cwd = typeof input.cwd === "string" && input.cwd ? input.cwd : undefined;

const passthrough = () => {
  writeJson({});
  process.exit(0);
};

const rawCmd =
  typeof toolInput.command === "string" ? toolInput.command : "";

if (input.tool_name !== "Bash" || !rawCmd) passthrough();

const caps = bunCapabilities();
const rewritten = rewriteCommand(rawCmd, {
  bunCheckout: isBunCheckout(cwd),
  aphrody: caps.uv,
});
const hint = preferHint(rewritten?.command ?? rawCmd);

if (!rewritten && !hint) passthrough();

const updated = rewritten
  ? { ...toolInput, command: rewritten.command }
  : undefined;
const note = [
  rewritten ? `Rewritten to Bun-native: ${rewritten.changes.join(", ")}.` : "",
  hint ?? "",
]
  .filter(Boolean)
  .join(" ");

const hookSpecificOutput: Record<string, unknown> = {
  hookEventName: "PreToolUse",
  permissionDecision: "allow",
};
if (updated) hookSpecificOutput.updatedInput = updated;
if (note) hookSpecificOutput.additionalContext = note;
writeJson({ hookSpecificOutput });
