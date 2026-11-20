import { expect, test } from "bun:test";
import {
  BASELINE,
  decide,
  serverFromPrefix,
  toolPrefix,
  transcriptDir,
} from "./mcp-profiles.ts";

test("tool prefixes round-trip for plugin servers, claude.ai connectors and built-ins", () => {
  for (const s of [
    "plugin:aphrody:ovh-cloud",
    "claude.ai Google Drive",
    "claude-in-chrome",
  ])
    expect(serverFromPrefix(toolPrefix(s))).toBe(s);
  expect(toolPrefix("claude.ai Figma")).toBe("mcp__claude_ai_Figma__");
});

test("transcript directories follow Claude Code's naming", () => {
  expect(transcriptDir("C:/src/browser")).toBe("C--src-browser");
});

test("a server is kept when baseline, used, or implied by the project, and disabled otherwise", () => {
  const universe = [
    ...BASELINE,
    "claude.ai Figma",
    "plugin:aphrody:ovh-vps",
    "plugin:aphrody:hostinger-dns",
    "claude.ai Gmail",
  ];
  const d = decide(
    universe,
    new Map([["claude.ai Gmail", 3]]),
    "C:/work/shenron",
    [],
  );
  expect(d.keep).toContain("claude.ai Gmail");
  expect(d.keep).toContain("plugin:aphrody:ovh-vps");
  expect(d.keep).toContain("plugin:aphrody:hostinger-dns");
  expect(d.disable).toEqual(["claude.ai Figma"]);
  for (const b of BASELINE) expect(d.keep).toContain(b);
});
