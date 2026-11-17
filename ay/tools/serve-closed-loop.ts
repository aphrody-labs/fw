#!/usr/bin/env bun
// Closed-loop load test of a Bun.serve handler with a fixed service time, to check Little's law and find the
// saturation throughput (docs/science/async.md). The server runs in a child process that this script kills.
//   bun tools/serve-closed-loop.ts [--spin-ms 0.2] [--seconds 3] [--levels 1,2,4,8,16,32,64]
import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: { "spin-ms": { type: "string", default: "0.2" }, seconds: { type: "string", default: "3" }, levels: { type: "string", default: "1,2,4,8,16,32,64" }, serve: { type: "boolean", default: false } },
});
const spinMs = Number(values["spin-ms"]);

if (values.serve) {
  const s = Bun.serve({
    port: 0,
    fetch() {
      const end = performance.now() + spinMs;
      while (performance.now() < end);
      return new Response("ok");
    },
  });
  console.log(s.port);
  await new Promise(() => {});
}

const child = Bun.spawn([process.execPath, import.meta.path, "--serve", "--spin-ms", String(spinMs)], { stdout: "pipe" });
const reader = child.stdout.getReader();
const port = Number(new TextDecoder().decode((await reader.read()).value).trim());
const url = `http://127.0.0.1:${port}/`;
const seconds = Number(values.seconds);
const rows = [];
try {
  for (const c of values.levels!.split(",").map(Number)) {
    const start = performance.now();
    const warm = start + 500;
    const end = start + 500 + seconds * 1000;
    let done = 0;
    let sumW = 0;
    await Promise.all(
      Array.from({ length: c }, async () => {
        while (performance.now() < end) {
          const t = performance.now();
          await (await fetch(url)).text();
          if (t >= warm) {
            done++;
            sumW += performance.now() - t;
          }
        }
      }),
    );
    const x = done / seconds;
    const w = sumW / done;
    rows.push({ clients: c, throughputRps: Math.round(x), meanLatencyMs: +w.toFixed(3), littleL: +((x * w) / 1000).toFixed(2) });
  }
} finally {
  child.kill();
}
console.log(JSON.stringify({ spinMs, bun: Bun.version, rows }, null, 1));
