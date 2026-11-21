#!/usr/bin/env bun
// Forwards HTTP requests to an MCP server and logs each exchange (request line, MCP headers, body, status, response):
//   bun tools/mcp-conformance/tee-http.ts <listen-port> <upstream-url> <log>
// Use it between a real client (claude, the official inspector) and `fw serve` to record the session.

import { appendFileSync, writeFileSync } from "node:fs";

const [port, upstream, log] = process.argv.slice(2);
if (!port || !upstream || !log) {
  console.error("usage: tee-http.ts <listen-port> <upstream-url> <log>");
  process.exit(2);
}
writeFileSync(log, "");
const mcpHeaders = (headers: Headers) =>
  [...headers].filter(([name]) => /^(mcp-|accept$|content-type$|origin$|host$)/i.test(name)).map(([name, value]) => `${name}: ${value}`).join("; ");

Bun.serve({
  hostname: "127.0.0.1",
  port: Number(port),
  idleTimeout: 255,
  async fetch(req) {
    const body = req.method === "POST" ? await req.text() : "";
    appendFileSync(log, `-> ${req.method} ${new URL(req.url).pathname} [${mcpHeaders(req.headers)}] ${body}\n`);
    const res = await fetch(`${upstream}`, { method: req.method, headers: req.headers, body: body || undefined });
    const [forClient, forLog] = res.body ? res.body.tee() : [null, null];
    if (forLog) {
      (async () => {
        const text = await new Response(forLog).text();
        appendFileSync(log, `<- ${res.status} [${res.headers.get("content-type") ?? ""}] ${text.trim().replaceAll("\n", " | ")}\n`);
      })();
    } else appendFileSync(log, `<- ${res.status}\n`);
    return new Response(forClient, { status: res.status, headers: res.headers });
  },
});
