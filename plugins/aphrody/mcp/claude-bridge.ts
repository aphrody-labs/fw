#!/usr/bin/env bun
// Launcher of the `claude-bridge` MCP server; the implementation is configs/claude/scripts/claude-bridge.ts of this repository.
import { resolve } from "node:path";

const bridge = await import(resolve(import.meta.dir, "..", "..", "..", "configs", "claude", "scripts", "claude-bridge.ts"));
await bridge.serve();
