import { randomUUID } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { connect } from "node:net";
import { homedir } from "node:os";
import { basename, join } from "node:path";

/** One running Claude Code session, as written by Claude Code to `<claude home>/sessions/<pid>.json`. */
export interface ClaudeSessionRecord {
  pid: number;
  sessionId: string;
  cwd: string;
  name?: string;
  kind?: string;
  status?: string;
  messagingSocketPath?: string;
  peerToken?: string;
  updatedAt?: number;
}

export interface DeliveryResult {
  target: string;
  sessionId: string;
  channel: "pipe" | "mailbox";
  ok: boolean;
  error?: string;
}

export interface MailItem {
  from: string;
  text: string;
  at: number;
}

/** Claude Code's configuration directory: CLAUDE_CONFIG_DIR, else ~/.claude. */
export function claudeHome(): string {
  return process.env.CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude");
}

function isPidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Live sessions, most recently updated first. A session whose process is gone is skipped. */
export function listLiveClaudeSessions(
  home = claudeHome(),
): ClaudeSessionRecord[] {
  const dir = join(home, "sessions");
  if (!existsSync(dir)) return [];
  const files = readdirSync(dir);
  const sessions: ClaudeSessionRecord[] = [];
  for (const file of files) {
    if (!/^\d+\.json$/.test(file)) continue;
    try {
      const record = JSON.parse(
        readFileSync(join(dir, file), "utf8"),
      ) as ClaudeSessionRecord;
      if (!record.pid || !record.sessionId || !isPidAlive(record.pid)) continue;
      const keyFile = files.find(
        (name) => name.startsWith(`${record.pid}.`) && name.endsWith(".key"),
      );
      let peerToken: string | undefined;
      if (keyFile) {
        try {
          peerToken = (
            JSON.parse(readFileSync(join(dir, keyFile), "utf8")) as {
              peerToken?: string;
            }
          ).peerToken;
        } catch {
          // A key file Claude Code is rewriting: the session is then reachable by mailbox only.
        }
      }
      sessions.push({ ...record, peerToken });
    } catch {
      // A record Claude Code is rewriting.
    }
  }
  return sessions.sort((a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
}

/** The session this process runs in (Claude Code exports CLAUDE_CODE_SESSION_ID to its tools and hooks). */
export function currentSessionId(): string | undefined {
  return (
    process.env.CLAUDE_CODE_SESSION_ID ?? process.env.CLAUDE_SESSION_ID
  );
}

/**
 * Writes the frames Claude Code's messaging pipe accepts: an auth frame with the session's peer token, then one user
 * frame wrapped in a cross-session-message element. The pipe sends no acknowledgement, so `ok` means "written and closed".
 */
export function pushPipeMessage(
  session: ClaudeSessionRecord,
  text: string,
  options: { sender?: string; urgent?: boolean; timeoutMs?: number } = {},
): Promise<{ ok: boolean; error?: string }> {
  const socketPath = session.messagingSocketPath;
  if (!socketPath) {
    return Promise.resolve({
      ok: false,
      error: "no messagingSocketPath on session",
    });
  }
  const sender = (options.sender ?? "notify").replace(/["<>]/g, "");
  const authFrame = session.peerToken
    ? `${JSON.stringify({ type: "auth", token: session.peerToken })}\n`
    : "";
  const userFrame = `${JSON.stringify({
    msgV: 1,
    msg_id: randomUUID(),
    type: "user",
    message: {
      role: "user",
      content: `<cross-session-message from-name="${sender}">\n${text.trim()}\n</cross-session-message>`,
    },
    priority: options.urgent ? "now" : "next",
    from: sender,
  })}\n`;

  return new Promise((resolve) => {
    let settled = false;
    const finish = (result: { ok: boolean; error?: string }) => {
      if (settled) return;
      settled = true;
      resolve(result);
    };
    const socket = connect({ path: socketPath });
    socket.setTimeout(options.timeoutMs ?? 3000, () => {
      socket.destroy();
      finish({ ok: false, error: "pipe timeout" });
    });
    socket.on("error", (error) => finish({ ok: false, error: error.message }));
    socket.on("connect", () => {
      socket.write(authFrame + userFrame);
      socket.end();
    });
    socket.on("close", () => finish({ ok: true }));
  });
}

/** Mailbox of one session: one file per message, delivered by the plugin hook on the session's next prompt. */
export function mailboxDir(sessionId: string, home = claudeHome()): string {
  return join(home, "mailbox", sessionId.replace(/[^A-Za-z0-9-]/g, "_"));
}

export function queueMail(
  sessionId: string,
  sender: string,
  text: string,
  home = claudeHome(),
): string {
  const dir = mailboxDir(sessionId, home);
  mkdirSync(dir, { recursive: true });
  const at = Date.now();
  const order = String(process.hrtime.bigint()).padStart(20, "0");
  const file = join(dir, `${order}-${randomUUID()}.json`);
  writeFileSync(file, JSON.stringify({ from: sender, text, at } satisfies MailItem));
  return file;
}

/** Returns the pending messages of a session, oldest first, and removes them. */
export function drainMail(sessionId: string, home = claudeHome()): MailItem[] {
  const dir = mailboxDir(sessionId, home);
  if (!existsSync(dir)) return [];
  const items: MailItem[] = [];
  for (const name of readdirSync(dir).sort()) {
    const file = join(dir, name);
    try {
      items.push(JSON.parse(readFileSync(file, "utf8")) as MailItem);
      rmSync(file, { force: true });
    } catch {
      // A message another reader is draining.
    }
  }
  return items;
}

export interface DeliverOptions {
  message: string;
  /** "all", a sessionId (or prefix), a pid, a session name (substring) or a cwd basename. */
  target?: string;
  /** Sender name; defaults to the name of the calling session, else "notify". */
  sender?: string;
  cwdFilter?: string;
  urgent?: boolean;
  /** Also queue the mailbox copy when the pipe delivery succeeded. */
  alsoMailbox?: boolean;
  home?: string;
}

function matches(
  session: ClaudeSessionRecord,
  target: string,
  cwdFilter: string | undefined,
  exclude: Set<string>,
): boolean {
  if (cwdFilter) {
    const want = cwdFilter.replaceAll("\\", "/").toLowerCase();
    if (!session.cwd.replaceAll("\\", "/").toLowerCase().startsWith(want))
      return false;
  }
  if (target === "all" || target === "*" || target === "workspace")
    return !exclude.has(session.sessionId);
  if (String(session.pid) === target) return true;
  if (session.sessionId === target || session.sessionId.startsWith(target))
    return true;
  if (session.name) {
    if (
      session.name === target ||
      session.name.toLowerCase().includes(target.toLowerCase())
    )
      return true;
  }
  return basename(session.cwd).toLowerCase() === target.toLowerCase();
}

/**
 * Delivers a message to live Claude Code sessions. Claude Code accepts a pipe message only from a sender it knows as a
 * live session, so a sender that is not one (a script, CI) goes to the mailbox, which names the sender and is drained by the
 * plugin's UserPromptSubmit hook. A broadcast never loops back to the sending session.
 */
export async function deliverRealtime(
  options: DeliverOptions,
): Promise<DeliveryResult[]> {
  const home = options.home ?? claudeHome();
  const target = (options.target ?? "all").trim();
  const sessions = listLiveClaudeSessions(home);
  const own = sessions.find((s) => s.sessionId === currentSessionId());
  const sender = options.sender ?? own?.name ?? "notify";
  const senderIsSession = sessions.some(
    (s) => s.name === sender || s.sessionId === sender,
  );
  const exclude = new Set(
    sessions
      .filter((s) => s.name === sender || s.sessionId === sender)
      .map((s) => s.sessionId),
  );
  if (own) exclude.add(own.sessionId);

  const results: DeliveryResult[] = [];
  const selected = sessions.filter((s) =>
    matches(s, target, options.cwdFilter, exclude),
  );
  for (const session of selected) {
    const label =
      session.name ?? `${basename(session.cwd)}-${session.sessionId.slice(0, 6)}`;
    let delivered = false;
    if (session.messagingSocketPath && senderIsSession) {
      const result = await pushPipeMessage(session, options.message, {
        sender,
        urgent: options.urgent,
      });
      delivered = result.ok;
      results.push({
        target: label,
        sessionId: session.sessionId,
        channel: "pipe",
        ok: result.ok,
        error: result.error,
      });
    }
    if (!delivered || options.alsoMailbox) {
      queueMail(session.sessionId, sender, options.message, home);
      results.push({
        target: label,
        sessionId: session.sessionId,
        channel: "mailbox",
        ok: true,
      });
    }
  }
  if (selected.length === 0 && /^[0-9a-f-]{8,}$/i.test(target)) {
    queueMail(target, sender, options.message, home);
    results.push({ target, sessionId: target, channel: "mailbox", ok: true });
  }
  return results;
}
