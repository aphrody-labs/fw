// headersHelper of Claude Code's "ai-os" MCP server (apps/agents/toolchain.toml, headers_helper): prints
// the Authorization header carrying the ai-os's local token (user's private file), never written to
// ~/.claude.json or in argv. No interactive OAuth consent: the ai-os accepts its fixed token locally.
import { aiOsToken } from "./server.ts";

// Write expected before exit: an immediate process.exit() can truncate a pipe on Windows.
await Bun.write(
  Bun.stdout,
  JSON.stringify({ Authorization: `Bearer ${await aiOsToken()}` }),
);
process.exitCode = 0;
