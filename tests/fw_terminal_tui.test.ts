import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  generateFwWtaConfig,
  getTerminalTuiStatus,
  verifyFwSelfTui,
} from "../src/terminal_tui.ts";

const ROOT = resolve(import.meta.dir, "..");

describe("Fw Intelligent Terminal (WTA) + tui-test integration", () => {
  test("tui-test and wta are resolved from their forks, not vendored in the repo", () => {
    const status = getTerminalTuiStatus(ROOT);
    expect(status.ok).toBe(true);
    expect(status.tuiTest.forkUrl).toBe("https://github.com/aphrody-labs/tui-test");
    expect(status.intelligentTerminalWta.forkUrl).toBe("https://github.com/aphrody-labs/intelligent-terminal");
    expect(status.intelligentTerminalWta.path).toContain(join("third_party", "intelligent-terminal", "tools", "wta"));
    expect(existsSync(join(ROOT, "core", "rs"))).toBe(false);
    expect(status.skillInstalled).toBe(true);
  });

  test("Fw WTA config routes ACP/MCP and Gemini Decision 1 autofix", () => {
    const cfg = generateFwWtaConfig(ROOT);
    expect(cfg.acpAgent).toBe("claude");
    expect(cfg.delegateModel).toBe("gemini-decision-1");
    expect(cfg.autoErrorDetectionEnabled).toBe(true);
    expect(cfg.autoFixEnabled).toBe(true);
    expect(cfg.sessionMcpTools).toContain("tui_test_verify");
    expect(cfg.sessionMcpTools).toContain("gemini_decision");
  });

  test("verifyFwSelfTui verifies Fw CLI output end-to-end", () => {
    const res = verifyFwSelfTui(ROOT);
    expect(res.ok).toBe(true);
    expect(res.matched).toBe(true);
    expect(res.exitCode).toBe(0);
    expect(res.snapshot).toContain("fwWorkflowOptimized");
  }, 20_000);
});
