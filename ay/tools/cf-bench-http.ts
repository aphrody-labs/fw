#!/usr/bin/env bun
// Benchmarks xh vs curl vs curl(--http3-only) vs wget2 vs bun fetch against live Cloudflare endpoints.
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { privateDir as privateRoot } from "./local.ts";

interface BenchRow {
  tool: string;
  available: boolean;
  runs: number;
  ok: number;
  minMs: number;
  p50Ms: number;
  p95Ms: number;
  meanMs: number;
  note: string;
}

const root = resolve(import.meta.dir, "..");
const targetUrl = Bun.argv[2] ?? "https://1.1.1.1/cdn-cgi/trace";
const iterations = Number(Bun.argv[3] ?? 5);

function percentile(sorted: number[], p: number): number {
  if (!sorted.length) return 0;
  const idx = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return Math.round(sorted[idx]! * 100) / 100;
}

function summarize(tool: string, times: number[], ok: number, runs: number, note: string): BenchRow {
  const sorted = [...times].sort((a, b) => a - b);
  const sum = sorted.reduce((a, b) => a + b, 0);
  return {
    tool,
    available: true,
    runs,
    ok,
    minMs: sorted.length ? Math.round(sorted[0]! * 100) / 100 : 0,
    p50Ms: percentile(sorted, 50),
    p95Ms: percentile(sorted, 95),
    meanMs: sorted.length ? Math.round((sum / sorted.length) * 100) / 100 : 0,
    note,
  };
}

async function benchCli(tool: string, cmd: string[], note: string): Promise<BenchRow> {
  if (!Bun.which(cmd[0]!)) {
    return { tool, available: false, runs: 0, ok: 0, minMs: 0, p50Ms: 0, p95Ms: 0, meanMs: 0, note: "not installed on host" };
  }
  const times: number[] = [];
  let ok = 0;
  for (let i = 0; i < iterations; i++) {
    const t0 = performance.now();
    const p = Bun.spawnSync(cmd, { stdout: "ignore", stderr: "pipe" });
    const dt = performance.now() - t0;
    if (p.exitCode === 0) {
      ok++;
      times.push(dt);
    }
  }
  return summarize(tool, times, ok, iterations, note);
}

async function benchBunFetch(url: string): Promise<BenchRow> {
  const times: number[] = [];
  let ok = 0;
  for (let i = 0; i < iterations; i++) {
    const t0 = performance.now();
    const res = await fetch(url);
    await res.text();
    const dt = performance.now() - t0;
    if (res.ok) {
      ok++;
      times.push(dt);
    }
  }
  return summarize("bun-fetch", times, ok, iterations, `Bun ${Bun.version} native fetch`);
}

const curlVer = Bun.which("curl") ? Bun.spawnSync(["curl", "--version"]).stdout.toString() : "";
const hasHttp3 = /\bHTTP3\b/i.test(curlVer);

const rows: BenchRow[] = [
  await benchBunFetch(targetUrl),
  await benchCli("xh", ["xh", "--ignore-stdin", "--check-status", "-b", "GET", targetUrl], "Rust reqwest/hyper"),
  await benchCli("curl", ["curl", "-sSf", "-o", process.platform === "win32" ? "NUL" : "/dev/null", targetUrl], "C libcurl HTTP/2"),
  hasHttp3
    ? await benchCli("curl-http3", ["curl", "--http3-only", "-sSf", "-o", process.platform === "win32" ? "NUL" : "/dev/null", targetUrl], "C libcurl QUIC/HTTP3")
    : { tool: "curl-http3", available: false, runs: 0, ok: 0, minMs: 0, p50Ms: 0, p95Ms: 0, meanMs: 0, note: "curl lacks HTTP3" },
  await benchCli("wget2", ["wget2", "-q", "-O", process.platform === "win32" ? "NUL" : "/dev/null", targetUrl], "C wget2 HTTP/2"),
];

const md = [
  `# HTTP Client Benchmark (${targetUrl}, ${iterations} runs)`,
  "",
  "| Tool | Available | OK | Min (ms) | p50 (ms) | p95 (ms) | Mean (ms) | Notes |",
  "| :--- | :---: | :---: | ---: | ---: | ---: | ---: | :--- |",
  ...rows.map((r) =>
    `| ${r.tool} | ${r.available ? "yes" : "no"} | ${r.ok}/${r.runs} | ${r.minMs} | ${r.p50Ms} | ${r.p95Ms} | ${r.meanMs} | ${r.note} |`,
  ),
  "",
].join("\n");

const outDir = privateRoot(root);
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, "bench-http.json"), JSON.stringify({ targetUrl, iterations, rows }, null, 2) + "\n");
console.log(md);
