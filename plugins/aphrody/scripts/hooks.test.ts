import { expect, test } from "bun:test";
import { join } from "node:path";

const hooks = join(import.meta.dir, "..", "hooks");

async function run(script: string, input: unknown): Promise<unknown> {
  const proc = Bun.spawn([process.execPath, "--no-install", "--no-env-file", join(hooks, script)], {
    stdin: new TextEncoder().encode(JSON.stringify(input)),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
  expect(code).toBe(0);
  return JSON.parse(out);
}

test("pre-tool-use rewrites a Bash command in the Claude Code output shape", async () => {
  const out = (await run("pre-tool-use.ts", { tool_name: "Bash", tool_input: { command: "npm install express" } })) as {
    hookSpecificOutput: { hookEventName: string; permissionDecision: string; updatedInput: { command: string } };
  };
  expect(out.hookSpecificOutput.hookEventName).toBe("PreToolUse");
  expect(out.hookSpecificOutput.permissionDecision).toBe("allow");
  expect(out.hookSpecificOutput.updatedInput.command).toBe("bun install express");
});

test("pre-tool-use passes other tools and unchanged commands through with an empty object", async () => {
  expect(await run("pre-tool-use.ts", { tool_name: "Read", tool_input: { file_path: "a.txt" } })).toEqual({});
  expect(await run("pre-tool-use.ts", { tool_name: "Bash", tool_input: { command: "ls" } })).toEqual({});
});

test("session-start adds the Bun context", async () => {
  const out = (await run("session-start.ts", {})) as { hookSpecificOutput: { hookEventName: string; additionalContext: string } };
  expect(out.hookSpecificOutput.hookEventName).toBe("SessionStart");
  expect(out.hookSpecificOutput.additionalContext).toContain("Bun-native rules");
});
