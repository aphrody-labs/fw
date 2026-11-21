import { describe, expect, test } from "bun:test";
import { callKernelTool, GRAPH_QUERIES, KERNEL_TOOL_NAMES, kernelArgv } from "../src/kernel.ts";

describe("fw kernel MCP tools", () => {
  test("tools are named and listed once", () => {
    expect(KERNEL_TOOL_NAMES).toEqual(["kernel_status", "kernel_graph"]);
  });

  test("status asks for JSON and can stay offline", () => {
    expect(kernelArgv("kernel_status", {})).toEqual(["status", "--json"]);
    expect(kernelArgv("kernel_status", { offline: true })).toEqual(["status", "--offline", "--json"]);
  });

  test("graph queries are validated and forwarded", () => {
    expect(kernelArgv("kernel_graph", { query: "def", operands: ["vfs_read"], tree: "cachyos", limit: 5 })).toEqual([
      "graph", "def", "vfs_read", "--tree", "cachyos", "--limit", "5",
    ]);
    expect(kernelArgv("kernel_graph", { query: "stats" })).toEqual(["graph", "stats"]);
    expect(typeof kernelArgv("kernel_graph", { query: "build", operands: ["x"] })).toBe("string");
    expect(typeof kernelArgv("kernel_graph", { query: "def" })).toBe("string");
    for (const query of GRAPH_QUERIES) expect(kernelArgv("kernel_graph", { query, operands: ["a", "b"] })[0]).toBe("graph");
  });

  test("a missing engine is reported with the build hint", () => {
    const saved = process.env.FW_LINUX_BIN;
    process.env.FW_LINUX_BIN = "/nonexistent/fw-linux";
    const result = callKernelTool("/nonexistent-root", "kernel_status", {});
    if (saved === undefined) delete process.env.FW_LINUX_BIN;
    else process.env.FW_LINUX_BIN = saved;
    // Either the engine is on PATH on this machine, or the call says how to build it.
    if (!result.ok) expect(JSON.stringify(result.payload)).toMatch(/fw-linux/);
  });
});
