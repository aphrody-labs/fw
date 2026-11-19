import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  generateFwWtaConfig,
  getTerminalTuiStatus,
  verifyFwSelfTui,
} from "../src/terminal_tui.ts";

const ROOT = resolve(import.meta.dir, "..");

describe("Fw Intelligent Terminal (WTA) + tui-test source-only integration", () => {
  test("source-only core/rs/tui-test and core/rs/wta are present without binary font/exe bloat", () => {
    const status = getTerminalTuiStatus(ROOT);
    expect(status.ok).toBe(true);
    expect(status.tuiTest.present).toBe(true);
    expect(status.tuiTest.files).toBeGreaterThan(80);
    expect(status.intelligentTerminalWta.present).toBe(true);
    expect(status.intelligentTerminalWta.files).toBeGreaterThan(150);
    expect(status.skillInstalled).toBe(true);
    // Verify heavy binary assets were stripped
    expect(existsSync(join(ROOT, "core/rs/tui-test/crates/tui-test/assets/jetbrains-mono"))).toBe(false);
    expect(existsSync(join(ROOT, "core/rs/tui-test/crates/tui-test/assets/nerd-fonts"))).toBe(false);
    expect(existsSync(join(ROOT, "core/rs/tui-test/crates/tui-test/assets/conpty/x64"))).toBe(false);
  });

  test("Fw WTA config routes ACP/MCP and Gemini Decision 1 autofix", () => {
    const cfg = generateFwWtaConfig(ROOT);
    expect(cfg.acpAgent).toBe("antigravity");
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
