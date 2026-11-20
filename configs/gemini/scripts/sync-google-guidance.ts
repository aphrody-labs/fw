#!/usr/bin/env bun
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { GEMINI_ROOT } from "./lib.ts";

const REPOS = [
  { name: "modern-web-guidance-src", url: "https://github.com/GoogleChrome/modern-web-guidance-src.git" },
  { name: "styleguide", url: "https://github.com/google/styleguide.git" },
] as const;

export function syncGoogleGuidance(): Record<string, { status: string; sha: string }> {
  const vendorRoot = join(GEMINI_ROOT, "vendor");
  const brainDir = join(GEMINI_ROOT, "brain", "google-guidance");
  mkdirSync(vendorRoot, { recursive: true });
  mkdirSync(brainDir, { recursive: true });

  const results: Record<string, { status: string; sha: string }> = {};
  for (const repo of REPOS) {
    const dir = join(vendorRoot, repo.name);
    let status = "synced";
    if (!existsSync(join(dir, ".git"))) {
      const clone = Bun.spawnSync(["git", "clone", "--depth", "1", repo.url, dir], { stdout: "pipe", stderr: "pipe" });
      status = clone.exitCode === 0 ? "cloned" : "clone_failed";
    } else {
      const pull = Bun.spawnSync(["git", "-C", dir, "pull", "--ff-only", "--depth", "1"], { stdout: "pipe", stderr: "pipe" });
      status = pull.exitCode === 0 ? "updated" : "cached";
    }
    const rev = Bun.spawnSync(["git", "-C", dir, "rev-parse", "--short=12", "HEAD"], { stdout: "pipe", stderr: "pipe" });
    const sha = rev.exitCode === 0 ? rev.stdout.toString().trim() : "unknown";
    results[repo.name] = { status, sha };
  }

  const indexMd = [
    "# Google Guidance & Styleguide Brain (Auto-Synced)",
    "",
    `- \`GoogleChrome/modern-web-guidance-src\`: \`${results["modern-web-guidance-src"]?.sha}\` (\`${results["modern-web-guidance-src"]?.status}\`) -> \`~/.gemini/vendor/modern-web-guidance-src\``,
    `- \`google/styleguide\`: \`${results["styleguide"]?.sha}\` (\`${results["styleguide"]?.status}\`) -> \`~/.gemini/vendor/styleguide\``,
    "",
    "Use these vendored upstream repositories as normative lint and architecture references for Web (HTML/CSS/JS/TS/WebGPU/Wasm) and Google language style guides (Go, C++, Python, TypeScript, Shell, JSON, Markdown).",
    "",
  ].join("\n");
  writeFileSync(join(brainDir, "INDEX.md"), indexMd, "utf8");
  return results;
}

if (import.meta.main) {
  console.log(JSON.stringify(syncGoogleGuidance(), null, 2));
}
