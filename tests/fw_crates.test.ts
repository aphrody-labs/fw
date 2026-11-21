import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { BUILD_HINT, CRATES_TOOL_NAMES, callCratesTool, crateEngine, toolArgv } from "../src/crates.ts";
import { handleJsonRpcMessage } from "../src/mcp_server.ts";

const ROOT = process.cwd();

describe("fw crates in the Bun plugin", () => {
  test("MCP tools/list exposes the five crates tools next to the fw tools", () => {
    // MCP 2026-07-28 is stateless: every request carries its protocol version and client capabilities in _meta.
    const _meta = { "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} };
    const res = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 1, method: "tools/list", params: { _meta } });
    const names = (res?.result as { tools: Array<{ name: string }> }).tools.map((t) => t.name);
    for (const n of ["crates_search", "crates_info", "crates_compare", "crates_awesome", "crates_stack"]) expect(names).toContain(n);
    expect(names).toContain("fw_status");
    expect(CRATES_TOOL_NAMES).toHaveLength(5);
  });

  test("tool arguments become engine arguments, missing ones are reported", () => {
    expect(toolArgv("crates_search", { query: "http server", category: "web-programming", limit: 5 })).toEqual([
      "search", "http server", "--category", "web-programming", "--limit", "5", "--json",
    ]);
    expect(toolArgv("crates_info", { name: "axum" })).toEqual(["info", "axum", "--offline", "--json"]);
    expect(toolArgv("crates_info", { name: "axum", live: true })).toEqual(["info", "axum", "--json"]);
    expect(toolArgv("crates_compare", { names: ["axum", "poem"] })).toEqual(["compare", "axum", "poem", "--offline", "--json"]);
    expect(toolArgv("crates_awesome", {})).toEqual(["awesome", "--json"]);
    expect(toolArgv("crates_stack", { goal: "cli with tui" })).toEqual(["stack", "cli with tui", "--json"]);
    expect(toolArgv("crates_search", {})).toBe("missing `query`");
    expect(toolArgv("crates_compare", { names: ["axum"] })).toBe("compare needs at least two crate names");
    expect(toolArgv("crates_nope", {})).toBe("unknown crates tool: crates_nope");
  });

  test("a tool call runs the engine against FW_CRATES_HOME, or says how to build it", () => {
    const home = mkdtempSync(join(tmpdir(), "fw-crates-"));
    const rec = (name: string, score: number) => ({
      name, version: "1.0.0", description: `${name} web framework`, created: "2020-01-01", last_release: "2026-11-01",
      downloads: 10, recent_downloads: 5000, dependents: 3, owners: 1, versions: 2, releases_365d: 2, categories: ["web-programming::http-server"],
      score, components: { downloads: 1, dependents: 1, recency: 1, maintenance: 1, license: 1, msrv: 1, security: 1 },
    });
    writeFileSync(join(home, "meta.json"), JSON.stringify({ score_version: 1, dump_date: "2026-11-21", dump_timestamp: "2026-11-21T02:00:22Z", crates_io_commit: "x", advisories: 0, crates_total: 2, crates_ranked: 2, categories: 1, total_downloads: 1, source: "fixture" }));
    writeFileSync(join(home, "categories.json"), JSON.stringify([{ slug: "web-programming::http-server", name: "HTTP server", description: "d", crates: 2 }]));
    writeFileSync(join(home, "ranked.jsonl"), [rec("alpha", 80), rec("beta", 70)].map((r) => JSON.stringify(r)).join("\n") + "\n");
    const prev = process.env.FW_CRATES_HOME;
    process.env.FW_CRATES_HOME = home;
    try {
      const res = callCratesTool(ROOT, "crates_search", { query: "web framework" });
      if (crateEngine(ROOT)) {
        expect(res.ok).toBe(true);
        const payload = res.payload as { op: string; dump_date: string; results: Array<{ name: string }> };
        expect(payload.op).toBe("search");
        expect(payload.dump_date).toBe("2026-11-21");
        expect(payload.results.map((r) => r.name)).toEqual(["alpha", "beta"]);
      } else {
        expect(res.ok).toBe(false);
        expect((res.payload as { error: string }).error).toBe(BUILD_HINT);
      }
    } finally {
      if (prev === undefined) delete process.env.FW_CRATES_HOME;
      else process.env.FW_CRATES_HOME = prev;
    }
  });
});
