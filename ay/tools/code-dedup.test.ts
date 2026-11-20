import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  alphaNormalizeCode,
  renderCodeMarkdown,
  renderCodeSummary,
  runCodeAudit,
  stripCommentsAndNormalize,
  treeSitterAlphaSignature,
} from "./code-dedup.ts";

const tempDirs: string[] = [];

function initGitRepo(root: string, files: Record<string, string>): string {
  for (const [rel, body] of Object.entries(files)) {
    const abs = join(root, rel);
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, body);
  }
  const git = (...a: string[]) => Bun.spawnSync(["git", "-C", root, ...a], { stdout: "ignore", stderr: "ignore" });
  git("init", "-q");
  git("config", "core.fsmonitor", "false"); // a global fsmonitor daemon would hold the temp dir open on Windows
  git("add", ".");
  return root;
}

function makeWorkspace(repos: Record<string, Record<string, string>>): string {
  const ws = mkdtempSync(join(tmpdir(), "cdedup-"));
  tempDirs.push(ws);
  for (const [repoName, files] of Object.entries(repos)) {
    const dir = join(ws, repoName);
    mkdirSync(dir, { recursive: true });
    initGitRepo(dir, files);
  }
  return ws;
}

afterEach(() => {
  for (const d of tempDirs.splice(0)) {
    try {
      rmSync(d, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
    } catch (e) {
      // Windows can keep a handle on a fresh repo (search indexer, antivirus) past the test; the OS temp dir is left for the system to clear.
      if ((e as NodeJS.ErrnoException).code !== "EBUSY") throw e;
    }
  }
});

describe("code-dedup AST and Tree-sitter normalization", () => {
  test("stripCommentsAndNormalize and alphaNormalizeCode match renamed Type-2 clones", () => {
    const fnA = `
      // compute hash
      export function parseChecksumA(input: string): string {
        let acc = 2166136261;
        for (let i = 0; i < input.length; i++) {
          acc ^= input.charCodeAt(i);
          acc = Math.imul(acc, 16777619);
        }
        return (acc >>> 0).toString(16);
      }
    `;
    const fnB = `
      /* another header */
      export function parseChecksumB(rawText: string): string {
        let hashVal = 99999999;
        for (let idx = 0; idx < rawText.length; idx++) {
          hashVal ^= rawText.charCodeAt(idx);
          hashVal = Math.imul(hashVal, 16777619);
        }
        return (hashVal >>> 0).toString(16);
      }
    `;
    expect(stripCommentsAndNormalize(fnA)).not.toBe(stripCommentsAndNormalize(fnB));
    expect(alphaNormalizeCode(fnA)).toBe(alphaNormalizeCode(fnB));
  });

  test("web-tree-sitter normalizes renamed Rust functions into identical AST signatures", async () => {
    const rustA = `
      pub fn compute_alpha(items: &[u32], seed: u32) -> u32 {
          let mut sum = seed;
          for x in items {
              sum = sum.wrapping_add(*x);
          }
          sum
      }
    `;
    const rustB = `
      pub fn compute_beta(values: &[u32], init: u32) -> u32 {
          let mut total = init;
          for v in values {
              total = total.wrapping_add(*v);
          }
          total
      }
    `;
    const sigA = await treeSitterAlphaSignature("rust", rustA);
    const sigB = await treeSitterAlphaSignature("rust", rustB);
    expect(sigA).toBe(sigB);
    expect(sigA).toContain("(function_item");
  });
});

describe("code-dedup cross-repo detection", () => {
  test("detects copied upstream blobs, verbatim duplicate first-party files, AST Type-2 clones, code-graph clones, and SQL duplicates", async () => {
    const upstreamModule = [
      'import { join } from "node:path";',
      'import { readFileSync } from "node:fs";',
      "export function decodeManifestBlock(rootDir: string, relFile: string): string[] {",
      "  const target = join(rootDir, relFile);",
      "  const text = readFileSync(target, \"utf8\");",
      "  const rows: string[] = [];",
      "  for (const line of text.split(\"\\n\")) {",
      "    const clean = line.trim();",
      "    if (clean && !clean.startsWith(\"#\")) rows.push(clean);",
      "  }",
      "  return rows;",
      "}",
      "export function formatManifestReport(rows: string[]): string {",
      "  return rows.map((r, i) => `${i + 1}:${r}`).join(\"\\n\");",
      "}",
    ].join("\n");

    const renamedCloneA = [
      'import { resolve } from "node:path";',
      'import { Database } from "bun:sqlite";',
      "export function normalizeTokenStream(rawInput: string, limit: number): string[] {",
      "  const acc: string[] = [];",
      "  for (const part of rawInput.split(\",\")) {",
      "    const cleaned = part.trim().toLowerCase();",
      "    if (cleaned.length > 0 && acc.length < limit) {",
      "      acc.push(cleaned);",
      "    }",
      "  }",
      "  return acc;",
      "}",
      'export const querySqlA = "SELECT id, hash FROM pages WHERE url = ?1";',
      "export function helperPath(d: string) { return resolve(d); }",
    ].join("\n");

    const renamedCloneB = [
      'import { resolve } from "node:path";',
      'import { Database } from "bun:sqlite";',
      "export function sanitizeWordList(sourceText: string, cap: number): string[] {",
      "  const output: string[] = [];",
      "  for (const segment of sourceText.split(\";\")) {",
      "    const trimmed = segment.trim().toLowerCase();",
      "    if (trimmed.length > 0 && output.length < cap) {",
      "      output.push(trimmed);",
      "    }",
      "  }",
      "  return output;",
      "}",
      'export const querySqlB = "SELECT id, hash FROM pages WHERE url = ?2";',
      "export function helperPath(dir: string) { return resolve(dir); }",
    ].join("\n");

    const ws = makeWorkspace({
      "repo-one": {
        "third_party/up-lib/src/decode.ts": upstreamModule,
        "packages/copied/src/decode.ts": upstreamModule,
        "packages/alpha/src/tokens.ts": renamedCloneA,
        "packages/shared/src/exact.ts": "export function exactSharedUtil(v: number): number {\n  const x = v * 3;\n  const y = x + 7;\n  return y ^ 0xff;\n}\n",
      },
      "repo-two": {
        "packages/beta/src/words.ts": renamedCloneB,
        "packages/shared-copy/src/exact.ts": "export function exactSharedUtil(v: number): number {\n  const x = v * 3;\n  const y = x + 7;\n  return y ^ 0xff;\n}\n",
      },
    });

    const dbFile = join(ws, "graph.sqlite");
    const report = await runCodeAudit({ scanRoot: ws, minLines: 5, dbPath: dbFile, verifyDifft: true });

    // 1. Upstream copied into first-party via Git blob SHA-1
    expect(report.upstreamCopies.some((u) => u.kind === "git-blob" && u.firstPartyBucket === "repo-one:packages/copied")).toBe(true);

    // 2. Verbatim first-party file duplicate
    expect(report.duplicateFiles.some((d) => d.key === "exact.ts" && d.kind === "git-blob")).toBe(true);

    // 3. Type-2 renamed AST function clone (symbols come from `ast-grep outline`, an optional tool)
    if (report.tools.astGrep) expect(report.astClones.some((c) => c.tier === "type2-renamed" && c.members.length >= 2)).toBe(true);

    // 4. SQL normalization duplicate
    expect(report.sqlDuplicates.length).toBeGreaterThanOrEqual(1);

    // 5. SQLite code-graph DB persisted (filled from the same outlines) + fuzzy filter works
    if (report.tools.astGrep) expect(Bun.file(dbFile).size).toBeGreaterThan(0);
    const filtered = await runCodeAudit({ scanRoot: ws, minLines: 5, fuzzyQuery: "exact.ts" });
    expect(filtered.duplicateFiles.map((d) => d.key)).toEqual(["exact.ts"]);

    expect(renderCodeMarkdown(report)).toContain("# Code Duplication & Copied Upstream Audit");
    expect(renderCodeSummary(report, { ezaTree: true })).toContain("code-dedup:");
  });
});
