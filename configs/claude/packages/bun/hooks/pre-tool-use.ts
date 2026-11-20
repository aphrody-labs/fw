#!/usr/bin/env bun
import {
  isBunCheckout,
  bunCapabilities,
  detectHarness,
  preferHint,
  readHookInput,
  writeJson,
} from "./common.ts";
import { rewriteCommand } from "./rewrite.ts";

const harness = detectHarness(Bun.argv[2]);
const input = await readHookInput();
const tool =
  typeof input.tool_name === "string"
    ? input.tool_name
    : typeof input.toolCall?.name === "string"
      ? input.toolCall.name
      : "";
const toolInput = (
  input.toolCall?.args && typeof input.toolCall.args === "object"
    ? input.toolCall.args
    : (input.tool_input ?? {})
) as Record<string, unknown>;
const cwd =
  typeof input.cwd === "string" && input.cwd
    ? input.cwd
    : Array.isArray(input.workspacePaths) &&
        typeof input.workspacePaths[0] === "string"
      ? input.workspacePaths[0]
      : undefined;

const isShellTool =
  tool === "Bash" ||
  tool === "run_command" ||
  tool === "run_shell_command" ||
  tool === "";
const cmdKey =
  typeof toolInput.command === "string"
    ? "command"
    : typeof toolInput.CommandLine === "string"
      ? "CommandLine"
      : "command";
const rawCmd =
  typeof toolInput[cmdKey] === "string" ? (toolInput[cmdKey] as string) : "";

if (!isShellTool || !rawCmd) {
  writeJson({});
  process.exit(0);
}

const caps = bunCapabilities();
const rewritten = rewriteCommand(rawCmd, {
  bunCheckout: isBunCheckout(cwd),
  aphrody: caps.uv,
});
const hint = preferHint(rewritten?.command ?? rawCmd);

if (!rewritten && !hint) {
  writeJson({});
  process.exit(0);
}

const updated = rewritten
  ? { ...toolInput, [cmdKey]: rewritten.command }
  : undefined;
const note = [
  rewritten ? `Rewritten to Bun-native: ${rewritten.changes.join(", ")}.` : "",
  hint ?? "",
]
  .filter(Boolean)
  .join(" ");

if (harness === "agy") {
  const out: Record<string, unknown> = { decision: "allow" };
  if (updated) out.overwrite = updated;
  if (note) out.reason = note;
  writeJson(out);
} else if (harness === "gemini") {
  const out: Record<string, unknown> = { decision: "allow" };
  if (updated) out.hookSpecificOutput = { tool_input: updated };
  if (note) out.systemMessage = note;
  writeJson(out);
} else {
  const hookSpecificOutput: Record<string, unknown> = {
    hookEventName: "PreToolUse",
    permissionDecision: "allow",
  };
  if (updated) hookSpecificOutput.updatedInput = updated;
  if (note) hookSpecificOutput.additionalContext = note;
  writeJson({ hookSpecificOutput });
}
