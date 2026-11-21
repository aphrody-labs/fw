#!/usr/bin/env bun
/**
 * Stdio MCP server `claude-bridge`: lists live Claude Code sessions and injects a message into them, on top of
 * lib/claude-peer.ts (messaging pipe, mailbox fallback). Newline-delimited JSON-RPC 2.0 as in the MCP stdio transport.
 */
import { deliverRealtime, listLiveClaudeSessions } from "./lib/claude-peer.ts";

export const SERVER_INFO = { name: "claude-bridge", version: "1.0.0" };
/** Newest first; initialize answers the client's version when it is one of these, else the newest. */
export const PROTOCOL_VERSIONS = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

interface Tool {
  name: string;
  title: string;
  description: string;
  inputSchema: Record<string, unknown>;
  run: (args: Record<string, unknown>) => unknown | Promise<unknown>;
}

export const TOOLS: Tool[] = [
  {
    name: "claude_sessions",
    title: "Live Claude Code sessions",
    description:
      "List running Claude Code sessions: pid, sessionId, name, cwd, status and whether the messaging pipe is available.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
    run: () =>
      listLiveClaudeSessions().map(({ pid, sessionId, name, cwd, status, messagingSocketPath }) => ({
        pid,
        sessionId,
        name,
        cwd,
        status,
        pipeReady: Boolean(messagingSocketPath),
      })),
  },
  {
    name: "claude_inject",
    title: "Inject a message into live Claude Code sessions",
    description:
      "Deliver a message to live Claude Code sessions through their native messaging pipe, with a mailbox fallback drained on the session's next prompt. A broadcast skips the calling session.",
    inputSchema: {
      type: "object",
      properties: {
        message: { type: "string", description: "Text to deliver." },
        target: {
          type: "string",
          description: "sessionId, name, pid, cwd basename or 'all' (default 'all').",
        },
        sender: {
          type: "string",
          description: "Sender name (default: the calling session's name, else 'notify').",
        },
        cwd: { type: "string", description: "Only sessions whose cwd starts with this path." },
        urgent: { type: "boolean", description: "Pipe priority 'now' instead of 'next'." },
      },
      required: ["message"],
      additionalProperties: false,
    },
    run: (args) => {
      const message = typeof args.message === "string" ? args.message.trim() : "";
      if (!message) throw new ToolInputError("message is required");
      return deliverRealtime({
        message,
        target: typeof args.target === "string" ? args.target : "all",
        sender: typeof args.sender === "string" ? args.sender : undefined,
        cwdFilter: typeof args.cwd === "string" ? args.cwd : undefined,
        urgent: args.urgent === true,
      });
    },
  },
];

class ToolInputError extends Error {}

type Json = Record<string, unknown>;
const ok = (id: unknown, result: unknown): Json => ({ jsonrpc: "2.0", id, result });
const fail = (id: unknown, code: number, message: string): Json => ({
  jsonrpc: "2.0",
  id,
  error: { code, message },
});

/** One JSON-RPC message in, the response out; notifications and responses from the client give undefined. */
export async function handle(message: Json): Promise<Json | undefined> {
  const { id, method } = message;
  const isRequest = id !== undefined && typeof method === "string";
  if (!isRequest) return undefined;
  const params = (message.params ?? {}) as Json;
  switch (method) {
    case "initialize": {
      const asked = typeof params.protocolVersion === "string" ? params.protocolVersion : "";
      return ok(id, {
        protocolVersion: PROTOCOL_VERSIONS.includes(asked) ? asked : PROTOCOL_VERSIONS[0],
        capabilities: { tools: { listChanged: false } },
        serverInfo: SERVER_INFO,
      });
    }
    case "ping":
      return ok(id, {});
    case "tools/list":
      return ok(id, {
        tools: TOOLS.map(({ name, title, description, inputSchema }) => ({
          name,
          title,
          description,
          inputSchema,
        })),
      });
    case "tools/call": {
      const tool = TOOLS.find((candidate) => candidate.name === params.name);
      if (!tool) return fail(id, -32602, `unknown tool: ${String(params.name)}`);
      try {
        const value = await tool.run((params.arguments ?? {}) as Json);
        return ok(id, {
          content: [{ type: "text", text: JSON.stringify(value, null, 2) }],
          structuredContent: Array.isArray(value) ? { items: value } : value,
          isError: false,
        });
      } catch (error) {
        const text = error instanceof Error ? error.message : String(error);
        return ok(id, { content: [{ type: "text", text }], isError: true });
      }
    }
    default:
      return fail(id, -32601, `method not found: ${method}`);
  }
}

/** Reads messages from stdin until it closes. */
export async function serve(): Promise<void> {
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of Bun.stdin.stream()) {
    buffer += decoder.decode(chunk, { stream: true });
    let newline = buffer.indexOf("\n");
    while (newline >= 0) {
      const line = buffer.slice(0, newline).trim();
      buffer = buffer.slice(newline + 1);
      newline = buffer.indexOf("\n");
      if (!line) continue;
      let response: Json | undefined;
      try {
        response = await handle(JSON.parse(line) as Json);
      } catch {
        response = fail(null, -32700, "parse error");
      }
      if (response) process.stdout.write(`${JSON.stringify(response)}\n`);
    }
  }
}

if (import.meta.main) await serve();
