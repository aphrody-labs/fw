import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createFairwindApiHandler } from "../src/api.ts";
import { renderCrossPlatformServices, runDaemonCycle } from "../src/daemon.ts";
import { FAIRWIND_MCP_TOOLS, handleJsonRpcMessage } from "../src/mcp_server.ts";
import { pluginStatus } from "../src/plugin_sync.ts";

const ROOT = process.cwd();

describe("fairwind unified plugin, MCP, API, and cross-platform daemon", () => {
  test("plugin directory has strict Antigravity manifest and migrated agy/learn/fairwind skills", () => {
    const status = pluginStatus(ROOT);
    expect(status.ok).toBe(true);
    expect(status.name).toBe("fairwind");
    expect(status.skills).toEqual([
      "agy-knowledge",
      "binary-analysis",
      "fairwind",
      "learn",
      "low-level-lto-aot",
      "reverse-engineer-anything",
      "static-recompile",
      "tree-sitter",
      "tui-test",
    ]);

    const manifest = JSON.parse(readFileSync(join(ROOT, "plugin", "plugin.json"), "utf8"));
    expect(manifest.name).toBe("fairwind");
    expect(manifest.$schema).toContain("antigravity.google");
    expect("license" in manifest).toBe(false);
    expect("keywords" in manifest).toBe(false);

    expect(existsSync(join(ROOT, "plugin", "mcp_config.json"))).toBe(true);
    expect(existsSync(join(ROOT, "plugin", "gemini-extension.json"))).toBe(true);
    expect(existsSync(join(ROOT, "plugin", "mcp", "fairwind-mcp.ts"))).toBe(true);
    expect(existsSync(join(ROOT, "plugin", "rules", "AGENTS.md"))).toBe(true);
    expect(existsSync(join(ROOT, "plugin", "skills", "agy-knowledge", "references", "agy-knowledge.verified.json"))).toBe(true);
  });

  test("unified MCP JSON-RPC server exposes all 9 first/third-party tools and executes calls", () => {
    const init = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 1, method: "initialize" });
    expect(init?.result).toBeDefined();

    const list = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 2, method: "tools/list" });
    const tools = (list?.result as { tools: typeof FAIRWIND_MCP_TOOLS }).tools;
    expect(tools.length).toBe(13);
    const names = tools.map((t) => t.name);
    for (const expected of [
      "fairwind_status",
      "fairwind_ast_parse",
      "fairwind_pe_inspect",
      "fairwind_target_scan",
      "fairwind_upgrade",
      "fairwind_bench",
      "fairwind_learn",
      "fairwind_service",
      "fairwind_bridge",
      "fairwind_tui_terminal",
    ]) {
      expect(names).toContain(expected);
    }

    const astCall = handleJsonRpcMessage(ROOT, {
      jsonrpc: "2.0",
      id: 3,
      method: "tools/call",
      params: { name: "fairwind_ast_parse", arguments: { lang: "rust", file: "native/fairwind-abi/src/lib.rs" } },
    });
    const astText = (astCall?.result as { content: Array<{ text: string }> }).content[0].text;
    const astJson = JSON.parse(astText);
    expect(astJson.ok).toBe(true);
    expect(astJson.hasError).toBe(false);

    const bridgeCall = handleJsonRpcMessage(ROOT, {
      jsonrpc: "2.0",
      id: 4,
      method: "tools/call",
      params: { name: "fairwind_bridge", arguments: { subsystem: "heretic" } },
    });
    const bridgeJson = JSON.parse((bridgeCall?.result as { content: Array<{ text: string }> }).content[0].text);
    expect(bridgeJson.ok).toBe(true);
    expect(bridgeJson.present).toBe(true);
  });

  test("unified HTTP API handler serves health, status, ast, and 404 cleanly", async () => {
    const handler = createFairwindApiHandler(ROOT);
    const healthResp = await handler(new Request("http://127.0.0.1:4379/health"));
    expect(healthResp.status).toBe(200);
    const health = (await healthResp.json()) as { ok: boolean; abiLoaded: boolean; abiVersion: number };
    expect(health.ok).toBe(true);
    expect(health.abiLoaded).toBe(true);
    expect(health.abiVersion).toBe(4);

    const astResp = await handler(new Request("http://127.0.0.1:4379/ast?lang=typescript&file=src/index.ts"));
    expect(astResp.status).toBe(200);
    const ast = (await astResp.json()) as { ok: boolean; hasError: boolean };
    expect(ast.ok).toBe(true);
    expect(ast.hasError).toBe(false);

    const notFound = await handler(new Request("http://127.0.0.1:4379/unknown"));
    expect(notFound.status).toBe(404);
  });

  test("cross-platform boot-startup auto-restart manifests render for Windows, Linux, and macOS", () => {
    const rendered = renderCrossPlatformServices(ROOT);
    expect(rendered.ok).toBe(true);
    expect(existsSync(rendered.manifests.windows)).toBe(true);
    expect(existsSync(rendered.manifests.linux)).toBe(true);
    expect(existsSync(rendered.manifests.macos)).toBe(true);

    const winXml = readFileSync(rendered.manifests.windows).toString("utf16le");
    expect(winXml).toContain("<LogonTrigger>");
    expect(winXml).toContain("<RestartOnFailure>");

    const linuxUnit = readFileSync(rendered.manifests.linux, "utf8");
    expect(linuxUnit).toContain("Restart=always");
    expect(linuxUnit).toContain("WantedBy=default.target");

    const macPlist = readFileSync(rendered.manifests.macos, "utf8");
    expect(macPlist).toContain("<key>RunAtLoad</key>");
    expect(macPlist).toContain("<key>KeepAlive</key>");
  });

  test("deterministic daemon single cycle records target/daemon-state.json", () => {
    const cycle = runDaemonCycle(ROOT, { autoApply: false, withLearn: false, syncPlugin: false });
    expect(cycle.ok).toBe(true);
    expect(existsSync(cycle.statePath)).toBe(true);
  });
});
