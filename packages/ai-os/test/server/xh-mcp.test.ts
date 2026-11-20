import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

type XhTool = {
  name: string;
  readOnly?: boolean;
  run: (args: Record<string, unknown>) => Promise<unknown>;
};

const XH_MCP = join(homedir(), ".gemini", "plugins", "aphrody", "mcp", "xh-mcp.ts");
const HAS_XH_MCP = existsSync(XH_MCP);
const mod = HAS_XH_MCP
  ? ((await import(XH_MCP)) as {
      respond: (msg: string) => Promise<Record<string, any> | undefined>;
      xhTools: () => XhTool[];
    })
  : undefined;

const respond = async (msg: string) => mod!.respond(msg);
const xhTools = (): XhTool[] => mod!.xhTools();

const call = async (name: string, args: Record<string, unknown> = {}) => {
  const tool = xhTools().find((t: XhTool) => t.name === name);
  if (!tool) throw new Error(`missing tool ${name}`);
  return tool.run(args);
};

describe.skipIf(!HAS_XH_MCP)("xh MCP tools", () => {
  test("tool list is stable and marks xh_request as non-read-only", () => {
    const tools = xhTools();
    expect(tools.map((t: XhTool) => t.name)).toEqual([
      "xh_docs",
      "xh_code_graph",
      "xh_use_cases",
      "xh_translate_curl",
      "xh_build_request",
      "xh_request",
    ]);
    expect(tools.filter((t: XhTool) => !t.readOnly).map((t: XhTool) => t.name)).toEqual([
      "xh_request",
    ]);
  });

  test("xh_docs returns options, request items, exit codes, and filtered queries", async () => {
    const codes = (await call("xh_docs", { section: "exit_codes" })) as any;
    expect(codes.exitCodes.length).toBe(7);
    expect(codes.exitCodes.find((c: any) => c.code === 4)?.meaning).toContain("4xx");

    const sig = (await call("xh_docs", { section: "options", query: "unstable-m-sig" })) as any;
    expect(sig.options.length).toBe(4);
  });

  test("xh_code_graph exposes Tree-sitter metrics, pipeline, god nodes, and modules", async () => {
    const summary = (await call("xh_code_graph", { view: "summary" })) as any;
    expect(summary.summary.nodes).toBe(3826);
    expect(summary.summary.edges).toBe(6714);
    expect(summary.godNodes[0].symbol).toBe("run");

    const mod = (await call("xh_code_graph", { view: "modules", query: "src/nested_json.rs" })) as any;
    expect(mod.modules.length).toBe(1);
    expect(mod.modules[0].file).toBe("src/nested_json.rs");
  });

  test("xh_use_cases retrieves structured recipes by category, query, and id", async () => {
    const all = (await call("xh_use_cases")) as any;
    expect(all.count).toBe(16);

    const one = (await call("xh_use_cases", { id: "rfc9421-message-signatures" })) as any;
    expect(one.category).toBe("security");
    expect(one.command).toContain("--unstable-m-sig-id");

    const missing = (await call("xh_use_cases", { id: "nonexistent" })) as any;
    expect(missing.error).toContain("unknown use case");
  });

  if (Bun.which("xh")) {
    test("xh_translate_curl and xh_build_request run offline without network", async () => {
      const curlRes = (await call("xh_translate_curl", {
        args: ["post", "https://api.example.com/items", "name=widget", "count:=3"],
      })) as any;
      expect(curlRes.exitCode).toBe(0);
      expect(curlRes.curl).toContain("curl -X POST https://api.example.com/items");
      expect(curlRes.warnings).toBe("");

      const built = (await call("xh_build_request", {
        args: ["post", "https://api.example.com/items", "name=widget", "count:=3"],
      })) as any;
      expect(built.exitCode).toBe(0);
      expect(built.request).toContain("POST /items HTTP/1.1");
      expect(built.request).toContain('{"name":"widget","count":3}');
    });

    test("xh_request connects to a local Bun.serve endpoint", async () => {
      const srv = Bun.serve({
        port: 0,
        fetch: async (req) => {
          const body = await req.json();
          return Response.json({ ok: true, echo: body, header: req.headers.get("x-test") });
        },
      });
      try {
        const res = (await call("xh_request", {
          args: ["-b", "post", `http://127.0.0.1:${srv.port}/ping`, "hello=world", "x-test:yes"],
        })) as any;
        expect(res.exitCode).toBe(0);
        expect(res.statusCategory).toBe("ok");
        const parsed = JSON.parse(res.stdout);
        expect(parsed).toEqual({ ok: true, echo: { hello: "world" }, header: "yes" });
      } finally {
        srv.stop(true);
      }
    });
  }
});

describe.skipIf(!HAS_XH_MCP)("xh MCP JSON-RPC protocol", () => {
  test("respond handles initialize, ping, tools/list, tools/call, and errors", async () => {
    expect(await respond("{bad json")).toMatchObject({ error: { code: -32700 } });
    expect(await respond(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "ping" }))).toEqual({
      jsonrpc: "2.0",
      id: 1,
      result: {},
    });
    expect(
      await respond(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })),
    ).toBeUndefined();
    expect(
      await respond(
        JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "nope" } }),
      ),
    ).toMatchObject({ error: { code: -32602 } });
  });

  test("stdio subprocess answers initialize and tools/list", async () => {
    const proc = Bun.spawn(
      [process.execPath, XH_MCP],
      { stdin: "pipe", stdout: "pipe", stderr: "pipe" },
    );
    proc.stdin.write(
      `${JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} })}\n` +
        `${JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list" })}\n`,
    );
    proc.stdin.end();
    const lines = (await new Response(proc.stdout).text())
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l));
    expect(lines[0].result.serverInfo.name).toBe("aphrody-xh");
    expect(lines[1].result.tools.map((t: any) => t.name)).toContain("xh_use_cases");
    expect(await proc.exited).toBe(0);
  });
});
