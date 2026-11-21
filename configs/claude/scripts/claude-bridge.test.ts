import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { handle, PROTOCOL_VERSIONS } from "./claude-bridge.ts";

describe("claude-bridge handle", () => {
  test("initialize answers the client's version when supported, else the newest", async () => {
    const asked = await handle({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18" } });
    expect(asked?.result).toMatchObject({ protocolVersion: "2025-06-18", serverInfo: { name: "claude-bridge" } });
    const unknown = await handle({ jsonrpc: "2.0", id: 2, method: "initialize", params: { protocolVersion: "1999-01-01" } });
    expect((unknown?.result as { protocolVersion: string }).protocolVersion).toBe(PROTOCOL_VERSIONS[0]);
  });

  test("notifications get no response", async () => {
    expect(await handle({ jsonrpc: "2.0", method: "notifications/initialized" })).toBeUndefined();
  });

  test("tools/list names both tools with object schemas", async () => {
    const listed = await handle({ jsonrpc: "2.0", id: 3, method: "tools/list" });
    const tools = (listed?.result as { tools: Array<{ name: string; inputSchema: { type: string } }> }).tools;
    expect(tools.map((tool) => tool.name)).toEqual(["claude_sessions", "claude_inject"]);
    expect(tools.every((tool) => tool.inputSchema.type === "object")).toBe(true);
  });

  test("unknown method and unknown tool are JSON-RPC errors", async () => {
    expect((await handle({ jsonrpc: "2.0", id: 4, method: "nope" }))?.error).toMatchObject({ code: -32601 });
    expect((await handle({ jsonrpc: "2.0", id: 5, method: "tools/call", params: { name: "nope" } }))?.error).toMatchObject({
      code: -32602,
    });
  });

  test("a tool failure is a result with isError, not a protocol error", async () => {
    const reply = await handle({ jsonrpc: "2.0", id: 6, method: "tools/call", params: { name: "claude_inject", arguments: {} } });
    expect(reply?.result).toMatchObject({ isError: true });
  });
});

describe("claude-bridge over stdio", () => {
  test("initialize, list and inject with no live session", async () => {
    const home = mkdtempSync(join(tmpdir(), "claude-bridge-test-"));
    try {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "t", version: "0" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "claude_sessions", arguments: {} } },
        { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "claude_inject", arguments: { message: "test", target: "all" } } },
      ]
        .map((line) => JSON.stringify(line))
        .join("\n");
      const run = Bun.spawnSync(["bun", "--no-install", "--no-env-file", join(import.meta.dir, "claude-bridge.ts")], {
        stdin: new TextEncoder().encode(`${input}\n`),
        env: { ...process.env, CLAUDE_CONFIG_DIR: home },
        stdout: "pipe",
        stderr: "pipe",
      });
      const lines = run.stdout.toString().trim().split("\n").map((line) => JSON.parse(line));
      expect(lines.map((line) => line.id)).toEqual([1, 2, 3]);
      expect(lines[0].result.protocolVersion).toBe("2025-11-25");
      expect(JSON.parse(lines[1].result.content[0].text)).toEqual([]);
      expect(lines[2].result.isError).toBe(false);
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });
});

describe("mailbox hook", () => {
  test("hands queued messages to the session once, then prints nothing", async () => {
    const { queueMail } = await import("./lib/claude-peer.ts");
    const home = mkdtempSync(join(tmpdir(), "claude-mailbox-hook-"));
    try {
      queueMail("hook-session", "ci", "build done", home);
      const hook = join(import.meta.dir, "..", "..", "..", "plugins", "aphrody", "hooks", "mailbox.ts");
      const run = () =>
        Bun.spawnSync(["bun", "--no-install", "--no-env-file", hook], {
          stdin: new TextEncoder().encode(JSON.stringify({ session_id: "hook-session", hook_event_name: "UserPromptSubmit" })),
          env: { ...process.env, CLAUDE_CONFIG_DIR: home },
          stdout: "pipe",
        }).stdout.toString();
      const first = JSON.parse(run());
      expect(first.hookSpecificOutput.hookEventName).toBe("UserPromptSubmit");
      expect(first.hookSpecificOutput.additionalContext).toBe("[Live peer message from ci]: build done");
      expect(run()).toBe("");
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });
});
