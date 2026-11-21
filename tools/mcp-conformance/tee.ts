#!/usr/bin/env bun
// Runs an MCP stdio server and copies every message in both directions to a log, so a session with a real client
// (claude, the official inspector) can be shown as a transcript:
//   bun tools/mcp-conformance/tee.ts <log> -- <server command…>
// The client sees the server unchanged: stdin, stdout and the exit code pass through; stderr is kept apart.

import { appendFileSync, writeFileSync } from "node:fs";

const sep = process.argv.indexOf("--");
const log = process.argv[2];
const command = process.argv.slice(sep + 1);
if (sep < 3 || !log || command.length === 0) {
  console.error("usage: tee.ts <log> -- <server command…>");
  process.exit(2);
}
writeFileSync(log, "");
const proc = Bun.spawn(command, { stdin: "pipe", stdout: "pipe", stderr: "inherit" });

function pump(source: ReadableStream<Uint8Array>, arrow: string, sink: (chunk: string) => void): Promise<void> {
  return (async () => {
    const decoder = new TextDecoder();
    let buffer = "";
    for await (const chunk of source) {
      const text = decoder.decode(chunk, { stream: true });
      sink(text);
      buffer += text;
      for (let nl = buffer.indexOf("\n"); nl >= 0; nl = buffer.indexOf("\n")) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        if (line) appendFileSync(log, `${arrow} ${line}\n`);
      }
    }
  })();
}

const input = pump(Bun.stdin.stream(), "->", (text) => {
  proc.stdin.write(text);
  proc.stdin.flush();
}).then(() => proc.stdin.end());
const output = pump(proc.stdout, "<-", (text) => process.stdout.write(text));
await Promise.all([input, output]);
process.exit(await proc.exited);
