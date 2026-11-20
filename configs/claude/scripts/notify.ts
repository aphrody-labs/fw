#!/usr/bin/env bun
import {
  deliverRealtime,
  listLiveClaudeSessions,
} from "../packages/aphrody/hooks-handlers/claude-peer.ts";

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

const opt = (flag: string): string | undefined => {
  const i = args.indexOf(flag);
  return i >= 0 ? args[i + 1] : undefined;
};

const target = opt("--to") ?? "all";
const sender = opt("--from") ?? "agy";
const cwdFilter = opt("--cwd");
const urgent = args.includes("--urgent");
const alsoQueueHook = args.includes("--hook");

const skip = new Set<number>();
for (const f of ["--to", "--from", "--cwd"]) {
  const i = args.indexOf(f);
  if (i >= 0) {
    skip.add(i);
    skip.add(i + 1);
  }
}
const text = args
  .filter((a, i) => !skip.has(i) && a !== "--urgent" && a !== "--hook")
  .join(" ")
  .trim();

if (!text) {
  console.error(
    "Usage: bun scripts/notify.ts [--list] [--to all|id|name|pid] [--from sender] [--cwd path] [--urgent] [--hook] <message>",
  );
  process.exit(1);
}

const res = await deliverRealtime({
  target,
  sender,
  cwdFilter,
  urgent,
  alsoQueueHook,
  message: text,
});
console.log(JSON.stringify(res, null, 2));
