#!/usr/bin/env bun
// Memory index CLI (SQLite FTS5). The MCP server `memory` of the aphrody plugin exposes the same functions.
//   bun scripts/memory.ts index [--sessions]
//   bun scripts/memory.ts search <words...> [--kind memory|instructions|digest|doc|session] [--limit N] [--json]
import {
  defaultSources,
  indexAll,
  type Kind,
  openIndex,
  search,
} from "../packages/aphrody/hooks-handlers/memory.ts";

export function parseArgs(argv: string[]): {
  cmd: string;
  words: string[];
  flags: Map<string, string | true>;
} {
  const flags = new Map<string, string | true>();
  const words: string[] = [];
  const valued = new Set(["--kind", "--limit"]);
  for (let i = 1; i < argv.length; i++) {
    const a = argv[i]!;
    if (valued.has(a)) flags.set(a, argv[++i] ?? "");
    else if (a.startsWith("--")) flags.set(a, true);
    else words.push(a);
  }
  return { cmd: argv[0] ?? "", words, flags };
}

if (import.meta.main) {
  const { cmd, words, flags } = parseArgs(Bun.argv.slice(2));
  const db = openIndex();
  try {
    if (cmd === "index") {
      console.log(
        JSON.stringify(
          indexAll(db, defaultSources(undefined, flags.has("--sessions"))),
        ),
      );
    } else if (cmd === "search" && words.length > 0) {
      indexAll(db, defaultSources());
      const hits = search(db, words.join(" "), {
        kind:
          typeof flags.get("--kind") === "string"
            ? (flags.get("--kind") as Kind)
            : undefined,
        limit: flags.has("--limit") ? Number(flags.get("--limit")) : undefined,
      });
      if (flags.has("--json")) console.log(JSON.stringify(hits, null, 2));
      else
        for (const h of hits)
          console.log(
            `[${h.kind}] ${h.title}\n  ${h.path}\n  ${h.snippet.replace(/\s+/g, " ")}`,
          );
    } else {
      console.error(
        "usage: bun scripts/memory.ts index [--sessions] | search <words...> [--kind K] [--limit N] [--json]",
      );
      process.exit(1);
    }
  } finally {
    db.close();
  }
}
