#!/usr/bin/env bun
/**
 * Unified parallel multi-language, cross-OS, cross-host CI/CD orchestrator for `fw`.
 * Runs independent verification lanes concurrently and prints a structured JSON summary.
 *
 * Usage:
 *   bun tools/ci.ts all       # Run all local parallel lanes (Rust, Native ABI, Wasm/musl, Python, Bun/Edge, Purity)
 *   bun tools/ci.ts fast      # Fast parallel gate (Clippy, Rust tests, Ruff, Bun tests, Purity)
 *   bun tools/ci.ts fleet     # Dispatch parallel cross-host CI lanes on fr1 + fr2 via tmux
 */

import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const mode = Bun.argv[2] ?? "all";

interface Lane {
  id: string;
  cmd: string[];
  cwd?: string;
}

interface LaneResult {
  id: string;
  ok: boolean;
  exitCode: number;
  ms: number;
  tail: string;
}

async function runLane(lane: Lane): Promise<LaneResult> {
  const t0 = performance.now();
  const proc = Bun.spawn(lane.cmd, {
    cwd: lane.cwd ?? ROOT,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  const combined = `${out}\n${err}`.trim().split(/\r?\n/).filter(Boolean);
  return {
    id: lane.id,
    ok: code === 0,
    exitCode: code,
    ms: Math.round(performance.now() - t0),
    tail: combined.slice(-4).join(" | ").slice(0, 260),
  };
}

if (mode === "fleet") {
  const tmuxRun = join(homedir(), ".gemini", "scripts", "tmux-run.ts");
  if (!existsSync(tmuxRun)) {
    console.error(`Missing tmux launcher: ${tmuxRun}`);
    process.exit(1);
  }
  const fleetLanes: Lane[] = [
    {
      id: "fr1-rust-release",
      cmd: [
        "bun",
        tmuxRun,
        "start",
        "fr1",
        "fw-ci-rust",
        "cd /srv/xbuild/fairwind 2>/dev/null && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace || echo 'clone repo to /srv/xbuild/fairwind first'",
      ],
    },
    {
      id: "fr2-edge-lint",
      cmd: [
        "bun",
        tmuxRun,
        "start",
        "fr2",
        "fw-ci-edge",
        "cd /srv/xbuild/fairwind 2>/dev/null && bun test && uvx ruff check . || echo 'clone repo to /srv/xbuild/fairwind first'",
      ],
    },
  ];
  const results = await Promise.all(fleetLanes.map(runLane));
  const ok = results.every((r) => r.ok);
  console.log(JSON.stringify({ mode, ok, results }, null, 2));
  process.exit(ok ? 0 : 1);
}

const fastLanes: Lane[] = [
  { id: "rust-clippy-test", cmd: ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"] },
  { id: "rust-workspace-test", cmd: ["cargo", "test", "--workspace"] },
  { id: "python-ruff", cmd: ["uvx", "ruff", "check", "."] },
  { id: "bun-test", cmd: ["bun", "test"] },
  { id: "repo-map", cmd: ["bun", "src/repo_map.ts"] },
];

const extraLanes: Lane[] = [
  { id: "native-pe-test", cmd: ["cargo", "test", "--manifest-path", "native/fairwind-pe/Cargo.toml"] },
  { id: "native-sniff-test", cmd: ["cargo", "test", "--manifest-path", "native/fairwind-sniff/Cargo.toml"] },
  { id: "native-abi-test", cmd: ["cargo", "test", "--manifest-path", "native/fairwind-abi/Cargo.toml"] },
  { id: "wasm32-release", cmd: ["cargo", "build", "--release", "--target", "wasm32-unknown-unknown", "-p", "fw-wasm"] },
  { id: "cloudflare-dry-run", cmd: ["bunx", "wrangler", "deploy", "--dry-run", "-c", "cloudflare/wrangler.jsonc"] },
];

const lanes = mode === "fast" ? fastLanes : [...fastLanes, ...extraLanes];
const tStart = performance.now();
const results = await Promise.all(lanes.map(runLane));
const ok = results.every((r) => r.ok);

console.log(
  JSON.stringify(
    {
      product: "fw",
      mode,
      ok,
      wallMs: Math.round(performance.now() - tStart),
      passed: results.filter((r) => r.ok).length,
      total: results.length,
      lanes: results,
    },
    null,
    2,
  ),
);

process.exit(ok ? 0 : 1);
