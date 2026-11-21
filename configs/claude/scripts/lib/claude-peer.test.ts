import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  deliverRealtime,
  drainMail,
  listLiveClaudeSessions,
  pushPipeMessage,
  queueMail,
} from "./claude-peer.ts";

let home: string;
let server: Server;
let pipePath: string;
const frames: string[] = [];

const writeSession = (
  pid: number,
  record: Record<string, unknown>,
  peerToken?: string,
) => {
  const dir = join(home, "sessions");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, `${pid}.json`), JSON.stringify({ pid, ...record }));
  if (peerToken)
    writeFileSync(join(dir, `${pid}.abc.key`), JSON.stringify({ peerToken }));
};

beforeAll(async () => {
  home = mkdtempSync(join(tmpdir(), "claude-peer-test-"));
  const id = crypto.randomUUID();
  pipePath =
    process.platform === "win32"
      ? `\\\\.\\pipe\\LOCAL\\cc-msg-test-${id}`
      : join(home, `cc-msg-${id}.sock`);
  server = createServer((socket) => {
    let buffer = "";
    socket.on("data", (chunk) => {
      buffer += chunk.toString();
    });
    socket.on("end", () => frames.push(...buffer.split("\n").filter(Boolean)));
  });
  await new Promise<void>((resolve) => server.listen(pipePath, resolve));
});

afterAll(() => {
  server.close();
  rmSync(home, { recursive: true, force: true });
});

describe("claude-peer", () => {
  test("lists only live sessions and attaches the peer token", () => {
    writeSession(
      process.pid,
      { sessionId: "live-1", cwd: "/w/live", name: "live", updatedAt: 2, messagingSocketPath: pipePath },
      "token-1",
    );
    writeSession(2_147_483_000, { sessionId: "dead-1", cwd: "/w/dead" });
    const sessions = listLiveClaudeSessions(home);
    expect(sessions.map((s) => s.sessionId)).toEqual(["live-1"]);
    expect(sessions[0]?.peerToken).toBe("token-1");
  });

  test("pushPipeMessage writes an auth frame then a user frame", async () => {
    frames.length = 0;
    const [session] = listLiveClaudeSessions(home);
    const result = await pushPipeMessage(session!, " hello ", {
      sender: "tester",
      urgent: true,
    });
    expect(result).toEqual({ ok: true });
    await Bun.sleep(50);
    expect(frames).toHaveLength(2);
    expect(JSON.parse(frames[0]!)).toEqual({ type: "auth", token: "token-1" });
    const user = JSON.parse(frames[1]!);
    expect(user).toMatchObject({ type: "user", priority: "now", from: "tester", msgV: 1 });
    expect(user.message.content).toBe(
      '<cross-session-message from-name="tester">\nhello\n</cross-session-message>',
    );
  });

  test("a session without a pipe is reported, not thrown", async () => {
    const result = await pushPipeMessage(
      { pid: 1, sessionId: "x", cwd: "/" },
      "hi",
    );
    expect(result.ok).toBe(false);
  });

  test("mailbox returns messages oldest first and empties", () => {
    queueMail("sess-a", "ci", "first", home);
    queueMail("sess-a", "ci", "second", home);
    expect(drainMail("sess-a", home).map((m) => m.text)).toEqual(["first", "second"]);
    expect(drainMail("sess-a", home)).toEqual([]);
  });

  test("deliverRealtime pipes for a session sender and mailboxes for any other", async () => {
    writeSession(process.ppid, { sessionId: "sender-1", cwd: "/w/sender", name: "sender", updatedAt: 1 });
    frames.length = 0;
    const piped = await deliverRealtime({ message: "m1", target: "live", sender: "sender", home });
    expect(piped).toEqual([
      expect.objectContaining({ channel: "pipe", ok: true, sessionId: "live-1" }),
    ]);
    const mailed = await deliverRealtime({ message: "m2", target: "live", sender: "script", home });
    expect(mailed).toEqual([
      expect.objectContaining({ channel: "mailbox", ok: true, sessionId: "live-1" }),
    ]);
    expect(drainMail("live-1", home).map((m) => m.text)).toEqual(["m2"]);
  });

  test("a broadcast skips the sending session", async () => {
    const results = await deliverRealtime({ message: "all", target: "all", sender: "sender", home });
    expect(results.map((r) => r.sessionId)).toEqual(["live-1"]);
  });
});
