#!/usr/bin/env bun
/**
 * Deterministic byte-exact repository map generator (`MAP.tsv`).
 * Gives AI agents the full structure, file counts, exact byte sizes, and gate commands
 * in < 600 tokens instead of scanning 9,000+ files.
 */

import { existsSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");

const ROLES: Record<string, { tier: string; gate: string }> = {
  "crates/fw-bin": { tier: "rust-core", gate: "cargo build --release -p fw-bin" },
  "crates/fw-cluster": { tier: "rust-core", gate: "cargo test -p fw-cluster" },
  "crates/fw-compute": { tier: "rust-core", gate: "cargo test -p fw-compute" },
  "crates/fw-ggml": { tier: "rust-core", gate: "cargo test -p fw-ggml" },
  "crates/fw-hermes-claw": { tier: "rust-core", gate: "cargo test -p fw-hermes-claw" },
  "crates/fw-nix": { tier: "rust-core", gate: "cargo test -p fw-nix" },
  "crates/fw-ostd-wasm": { tier: "rust-core", gate: "cargo test -p fw-ostd-wasm" },
  "crates/fw-proto": { tier: "rust-core", gate: "cargo test -p fw-proto" },
  "crates/fw-ui-blitz": { tier: "rust-core", gate: "cargo test -p fw-ui-blitz" },
  "crates/fw-unified-stack": { tier: "rust-core", gate: "cargo test -p fw-unified-stack" },
  "crates/fw-uutils": { tier: "rust-core", gate: "cargo test -p fw-uutils" },
  "crates/fw-wasm": { tier: "rust-wasm", gate: "cargo build --release --target wasm32-unknown-unknown -p fw-wasm" },
  "crates/fw-wayland-wgpu": { tier: "rust-core", gate: "cargo test -p fw-wayland-wgpu" },
  "native": { tier: "rust-ffi-abi", gate: "cargo test --manifest-path native/fairwind-abi/Cargo.toml" },
  "os/fairwind-os": { tier: "asterinas-osdk", gate: "bun test tests/fairwind_v4_os_uutils_nix.test.ts" },
  "os/fairwind-wsl": { tier: "wsl-zero-legacy", gate: "bun test tests/fairwind_v4_os_uutils_nix.test.ts" },
  "nix": { tier: "nix-fleet", gate: "bun test tests/fairwind_v4_os_uutils_nix.test.ts" },
  "cloudflare": { tier: "edge-worker", gate: "bun run cf:dry-run" },
  "vercel": { tier: "edge-vercel", gate: "bun test tests/fairwind_decisions_vercel.test.ts" },
  "web": { tier: "control-plane-ui", gate: "bun run cf:dry-run" },
  "src": { tier: "bun-cli-aot", gate: "bun test" },
  "tests": { tier: "e2e-tests", gate: "bun test" },
  "plugin": { tier: "agy-plugin", gate: "dist/fairwind.exe plugin sync --install" },
  "cluster": { tier: "topology-spec", gate: "bun test tests/fairwind_v3_cluster_webgpu.test.ts" },
  "configs/heretic": { tier: "heretic-presets", gate: "uvx ruff check ." },
  "docs": { tier: "architecture-docs", gate: "read-only" },
  "core/py": { tier: "python-core", gate: "uvx ruff check ." },
  "core/rs": { tier: "vendored-rs", gate: "ignored-by-default (.rgignore)" },
  "core/ts": { tier: "vendored-ts", gate: "ignored-by-default (.rgignore)" },
};

const tracked = Bun.spawnSync(["git", "-C", ROOT, "ls-files"], { stdout: "pipe" })
  .stdout.toString()
  .split(/\r?\n/)
  .filter(Boolean);

const buckets = new Map<string, { files: number; bytes: number }>();

function bucketFor(rel: string): string {
  const parts = rel.split("/");
  if (parts.length === 1) return "(root)";
  const p2 = `${parts[0]}/${parts[1]}`;
  if (ROLES[p2]) return p2;
  if (ROLES[parts[0]]) return parts[0];
  return p2;
}

for (const rel of tracked) {
  const abs = join(ROOT, rel);
  if (!existsSync(abs)) continue;
  const st = statSync(abs);
  if (!st.isFile()) continue;
  const k = bucketFor(rel);
  const b = buckets.get(k) ?? { files: 0, bytes: 0 };
  b.files++;
  b.bytes += st.size;
  buckets.set(k, b);
}

const rows = [
  "dir\ttier\tfiles\tbytes\tgate",
  ...[...buckets.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([dir, v]) => {
      const meta = ROLES[dir] ?? { tier: "misc", gate: "just ci" };
      return `${dir}\t${meta.tier}\t${v.files}\t${v.bytes}\t${meta.gate}`;
    }),
];

const outPath = join(ROOT, "MAP.tsv");
writeFileSync(outPath, `${rows.join("\n")}\n`, "utf8");
console.log(JSON.stringify({ ok: true, outPath, rows: rows.length - 1, bytes: Buffer.byteLength(rows.join("\n") + "\n") }));
