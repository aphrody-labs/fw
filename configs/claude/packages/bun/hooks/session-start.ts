#!/usr/bin/env bun
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { bunCapabilities, detectHarness, writeJson } from "./common.ts";

const harness = detectHarness(Bun.argv[2]);
if (!process.stdin.isTTY) {
  await new Response(Bun.stdin.stream()).text();
}

const caps = bunCapabilities();
const header = [
  `bun ${Bun.version_with_sha} at ${process.execPath}.`,
  `Native checks: bun check, bun lint, bun fmt${caps.uv ? ", bun uv" : ""}${caps.mcp ? ", bun mcp" : ""}.`,
];

const ctxPath = join(import.meta.dir, "context.md");
let body = "";
if (existsSync(ctxPath)) {
  body = readFileSync(ctxPath, "utf8").trim();
}
const msg = `${header.join(" ")}\n\n${body}`;

if (harness === "agy") {
  writeJson({ injectSteps: [{ systemMessage: { systemMessage: msg } }] });
} else {
  writeJson({
    hookSpecificOutput: {
      hookEventName: "SessionStart",
      additionalContext: msg,
    },
  });
}
