#!/usr/bin/env bun
import { deliverRealtime, listLiveClaudeSessions } from "./lib/claude-peer.ts";

const USAGE =
  "Usage: bun scripts/notify.ts [--list] [--to all|id|name|pid|cwd-basename] [--from sender] [--cwd path] [--urgent] [--mailbox] <message>";

const args = Bun.argv.slice(2);
if (args.includes("--list") || args.includes("-l")) {
  const rows = listLiveClaudeSessions().map(
    ({ pid, sessionId, name, cwd, status, messagingSocketPath }) => ({
      pid,
      sessionId,
      name,
      cwd,
      status,
      pipeReady: Boolean(messagingSocketPath),
    }),
  );
  console.log(JSON.stringify(rows, null, 2));
  process.exit(0);
}

const valueFlags = ["--to", "--from", "--cwd"];
const option = (flag: string): string | undefined => {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
};
const text = args
  .filter(
    (arg, index) =>
      !valueFlags.includes(arg) &&
      !valueFlags.includes(args[index - 1] ?? "") &&
      arg !== "--urgent" &&
      arg !== "--mailbox",
  )
  .join(" ")
  .trim();

if (!text) {
  console.error(USAGE);
  process.exit(1);
}

const results = await deliverRealtime({
  message: text,
  target: option("--to") ?? "all",
  sender: option("--from"),
  cwdFilter: option("--cwd"),
  urgent: args.includes("--urgent"),
  alsoMailbox: args.includes("--mailbox"),
});
console.log(JSON.stringify(results, null, 2));
process.exit(results.some((result) => !result.ok) ? 1 : 0);
