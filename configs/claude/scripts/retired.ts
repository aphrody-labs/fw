#!/usr/bin/env bun
/**
 * Fails when a tracked file mentions a retired surface (relay hub, fr2 as a host, agy tooling).
 * Terms and allowed paths come from configs/claude/retired.toml.
 *
 *   bun configs/claude/scripts/retired.ts --check [--config path] [--root dir]
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

export interface Term {
  id: string;
  pattern: string;
  reason: string;
}
export interface Allow {
  paths: string[];
  terms?: string[];
  reason: string;
}
export interface Config {
  term: Term[];
  allow: Allow[];
}
export interface Hit {
  file: string;
  line: number;
  term: string;
  text: string;
}

export function parseConfig(text: string): Config {
  const parsed = Bun.TOML.parse(text) as Partial<Config>;
  return { term: parsed.term ?? [], allow: parsed.allow ?? [] };
}

/** Scans `files`; `read` returns the text of a file, or undefined for one that is binary or unreadable. */
export function scan(
  files: string[],
  read: (file: string) => string | undefined,
  config: Config,
): Hit[] {
  const terms = config.term.map((term) => ({
    id: term.id,
    regex: new RegExp(term.pattern, "i"),
  }));
  const allowed = config.allow.map((entry) => ({
    globs: entry.paths.map((path) => new Bun.Glob(path)),
    terms: entry.terms,
  }));
  const hits: Hit[] = [];
  for (const file of files) {
    const applicable = terms.filter(
      (term) =>
        !allowed.some(
          (entry) =>
            (entry.terms === undefined || entry.terms.includes(term.id)) &&
            entry.globs.some((glob) => glob.match(file)),
        ),
    );
    if (applicable.length === 0) continue;
    const text = read(file);
    if (text === undefined) continue;
    text.split(/\r?\n/).forEach((line, index) => {
      for (const term of applicable) {
        if (term.regex.test(line))
          hits.push({
            file,
            line: index + 1,
            term: term.id,
            text: line.trim().slice(0, 160),
          });
      }
    });
  }
  return hits;
}

export function trackedFiles(root: string): string[] {
  const out = Bun.spawnSync(["git", "-C", root, "ls-files", "-z"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  if (out.exitCode !== 0)
    throw new Error(`git ls-files failed: ${out.stderr.toString().trim()}`);
  return out.stdout.toString().split("\0").filter(Boolean);
}

/** The text of a working-tree file, or undefined when it is missing, too large or binary (a NUL byte in the first 8 KiB). */
export function readText(root: string, file: string): string | undefined {
  const handle = Bun.file(join(root, file));
  if (handle.size === 0 || handle.size > 8 * 1024 * 1024) return undefined;
  try {
    const bytes = new Uint8Array(
      readFileSync(join(root, file)),
    );
    if (bytes.subarray(0, 8192).includes(0)) return undefined;
    return new TextDecoder().decode(bytes);
  } catch {
    return undefined;
  }
}

export function check(root: string, configPath: string): Hit[] {
  const config = parseConfig(
    readFileSync(configPath, "utf8"),
  );
  return scan(trackedFiles(root), (file) => readText(root, file), config);
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const option = (flag: string): string | undefined => {
    const index = args.indexOf(flag);
    return index >= 0 ? args[index + 1] : undefined;
  };
  const root =
    option("--root") ??
    Bun.spawnSync(["git", "-C", import.meta.dir, "rev-parse", "--show-toplevel"])
      .stdout.toString()
      .trim();
  const configPath = option("--config") ?? join(import.meta.dir, "..", "retired.toml");
  const hits = check(root, configPath);
  for (const hit of hits)
    console.error(`${hit.file}:${hit.line}: [${hit.term}] ${hit.text}`);
  if (hits.length > 0) {
    const files = new Set(hits.map((hit) => hit.file)).size;
    console.error(
      `FAIL retired surfaces: ${hits.length} line(s) in ${files} file(s); terms and allowed paths: ${configPath}`,
    );
    process.exit(1);
  }
  console.log("PASS retired surfaces: no retired term outside the allowed paths");
}
