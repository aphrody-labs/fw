import { describe, expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createFwApiHandler } from "../src/api.ts";
import { renderCrossPlatformServices, runDaemonCycle } from "../src/daemon.ts";
import { FW_MCP_TOOLS, handleJsonRpcMessage } from "../src/mcp_server.ts";
import { pluginStatus } from "../src/plugin_sync.ts";

const ROOT = process.cwd();

describe("fw unified plugin, MCP, API, and cross-platform daemon", () => {
  test("plugin directory is the unified Claude plugin: manifest, hooks, MCP servers and skills on disk", () => {
    const status = pluginStatus(ROOT);
    expect(status.ok).toBe(true);
    expect(status.name).toBe("aphrody");
    const pluginDir = join(ROOT, "plugins", "aphrody");
    const onDisk = readdirSync(join(pluginDir, "skills"), { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .map((d) => d.name)
      .sort();
    expect(status.skills).toEqual(onDisk);
    for (const core of ["fw", "hermes-claw", "learn", "low-level-lto-aot", "rust-upstream", "tree-sitter", "tui-test", "upstreams"]) {
      expect(status.skills).toContain(core);
    }
    expect(status.dedupe.overlapWithAphrody).toEqual([]);

    const manifest = JSON.parse(readFileSync(join(pluginDir, ".claude-plugin", "plugin.json"), "utf8"));
    expect(manifest.name).toBe("aphrody");
    for (const key of ["hooks", "mcpServers", "skills"]) {
      expect(existsSync(join(pluginDir, manifest[key]))).toBe(true);
    }
    const servers = JSON.parse(readFileSync(join(pluginDir, ".claude-plugin", "mcp.json"), "utf8")).mcpServers as Record<string, { args?: string[] }>;
    expect(Object.keys(servers)).toContain("fw");
    for (const server of Object.values(servers)) {
      for (const arg of server.args ?? []) {
        if (arg.startsWith("${CLAUDE_PLUGIN_ROOT}/")) expect(existsSync(join(pluginDir, arg.slice("${CLAUDE_PLUGIN_ROOT}/".length)))).toBe(true);
      }
    }
    // agy is retired: no Antigravity manifest or gemini path ships in the plugin.
    for (const retired of ["plugin.json", "gemini-extension.json", "mcp_config.json", join("skills", "agy-knowledge")]) {
      expect(existsSync(join(pluginDir, retired))).toBe(false);
    }
  });

  test("unified MCP JSON-RPC server serves both protocol eras, lists its tools and executes calls", () => {
    const init = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25" } });
    expect((init?.result as { protocolVersion: string }).protocolVersion).toBe("2025-11-25");

    const list = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 2, method: "tools/list" });
    const tools = (list?.result as { tools: typeof FW_MCP_TOOLS }).tools;
    expect(tools.length).toBe(FW_MCP_TOOLS.length);
    expect(tools.length).toBeGreaterThan(0);
    const names = tools.map((t) => t.name);
    expect(new Set(names).size).toBe(names.length);
    for (const expected of ["fw_status", "fw_ast_parse", "fw_pe_inspect", "fw_target_scan", "fw_learn", "fw_bridge", "fw_tui_terminal"]) {
      expect(names).toContain(expected);
    }

    // Every listed tool is callable: an empty call answers with a result or a validation error, never "Unknown tool".
    for (const name of names) {
      const res = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 9, method: "tools/call", params: { name, arguments: { action: "no-such-action" } } });
      expect(res?.error?.message ?? "").not.toContain("Unknown tool");
    }

    const meta = { "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} };
    const discover = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 5, method: "server/discover", params: { _meta: meta } });
    expect((discover?.result as { supportedVersions: string[] }).supportedVersions[0]).toBe("2026-07-28");
    const modernList = handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 6, method: "tools/list", params: { _meta: meta } });
    const modernResult = modernList?.result as { resultType: string; ttlMs: number; tools: unknown[] };
    expect(modernResult.resultType).toBe("complete");
    expect(modernResult.tools.length).toBe(tools.length);
    expect(modernResult.ttlMs).toBeGreaterThanOrEqual(0);
    expect(handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 7, method: "tools/list", params: { _meta: { ...meta, "io.modelcontextprotocol/protocolVersion": "1900-01-01" } } })?.error?.code).toBe(-32022);
    expect(handleJsonRpcMessage(ROOT, { jsonrpc: "2.0", id: 8, method: "nope", params: { _meta: meta } })?.error?.code).toBe(-32601);

    const astCall = handleJsonRpcMessage(ROOT, {
      jsonrpc: "2.0",
      id: 3,
      method: "tools/call",
      params: { name: "fw_ast_parse", arguments: { lang: "rust", file: "native/fw-abi/src/lib.rs" } },
    });
    const astText = (astCall?.result as { content: Array<{ text: string }> }).content[0].text;
    const astJson = JSON.parse(astText);
    expect(astJson.ok).toBe(true);
    expect(astJson.hasError).toBe(false);

    const bridgeCall = handleJsonRpcMessage(ROOT, {
      jsonrpc: "2.0",
      id: 4,
      method: "tools/call",
      params: { name: "fw_bridge", arguments: { subsystem: "heretic" } },
    });
    const bridgeJson = JSON.parse((bridgeCall?.result as { content: Array<{ text: string }> }).content[0].text);
    expect(bridgeJson.ok).toBe(true);
  });

  test("unified HTTP API handler serves health, status, ast, and 404 cleanly", async () => {
    const handler = createFwApiHandler(ROOT);
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
