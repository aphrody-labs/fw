#!/usr/bin/env bun
// Auto CLI and MCP stdio entrypoint: the `auto-cli` binary, and `runCli` for library use.
import { createAutoClientFromEnv } from "@aphrody/auto-client";
import { t, type Locale } from "@aphrody/auto-i18n";
import { llmConfigFromEnv, openAiClient, type LlmMessage } from "@aphrody/auto-platform/llm";
import {
  authGenerateApiKey,
  authHashPassword,
  authJwks,
  authTotpCode,
  authVerifyPassword,
  initNative,
} from "@aphrody/auto-native";
import { handleMcpJsonRpc, importLegacyN8nWorkflow, runAgentTurn, type JsonRpcRequest } from "@aphrody/auto-services";

const HELP = `auto-cli: Auto command line, agent and MCP stdio server

Usage:
  auto-cli mcp                                   MCP server on stdin/stdout (JSON-RPC 2.0), forwarding tool calls to the Auto server
  auto-cli tools                                 List the MCP tools of the Auto server
  auto-cli agent [prompt...]                     One-shot prompt, or an interactive session without arguments (LLM + MCP tools)
  auto-cli workflows list                        List the automations
  auto-cli workflows import <file.json>          Save an automation from a JSON file (n8n exports are converted)
  auto-cli workflows run <id> [json]             Run an automation with optional JSON input
  auto-cli auth hash <password>                  Argon2id hash of a password
  auto-cli auth verify <password> <hash>         Check a password against a hash
  auto-cli auth keygen [prefix]                  Generate an API key (default prefix auto_)
  auto-cli auth totp <secret>                    Current TOTP code of a secret
  auto-cli auth jwks <master-secret>             JWKS derived from a master secret
  auto-cli health                                Health of the configured Auto server

Environment:
  AUTO_API_URL     Base URL (default https://auto.aphrody.com)
  AUTO_UPSTREAMS   Comma-separated base URLs with health-based failover; replaces AUTO_API_URL
  AUTO_TOKEN       API key (auto_...) or bearer token
  AUTO_COMPANY_ID  Active company ID
  AUTO_LLM_URL     OpenAI-compatible LLM base URL for 'agent'
  AUTO_LLM_MODEL   Model name
  AUTO_LLM_KEY     Optional API key
  AUTO_LOCALE      Message language: en (default) or fr`;

async function ensureWasm(): Promise<void> {
  const wasmUrl = new URL("../../../packages/auto-native/wasm/auto_ffi_bg.wasm", import.meta.url);
  if (await Bun.file(wasmUrl).exists()) {
    initNative(await Bun.file(wasmUrl).bytes());
  }
}

export async function runCli(argv = process.argv.slice(2)): Promise<number> {
  const [cmd, ...rest] = argv;
  const locale: Locale = process.env.AUTO_LOCALE === "fr" ? "fr" : "en";

  if (!cmd || cmd === "help" || cmd === "--help" || cmd === "-h") {
    console.log(HELP);
    return 0;
  }

  if (cmd === "health") {
    const client = createAutoClientFromEnv();
    console.log(JSON.stringify(await client.health(), null, 2));
    return 0;
  }

  if (cmd === "auth") {
    await ensureWasm();
    const [sub, a, b] = rest;
    if (sub === "hash" && a) {
      console.log(authHashPassword(a));
      return 0;
    }
    if (sub === "verify" && a && b) {
      console.log(JSON.stringify({ valid: authVerifyPassword(a, b) }));
      return 0;
    }
    if (sub === "keygen") {
      console.log(JSON.stringify(authGenerateApiKey(a || "auto_"), null, 2));
      return 0;
    }
    if (sub === "totp" && a) {
      console.log(JSON.stringify(authTotpCode(a), null, 2));
      return 0;
    }
    if (sub === "jwks" && a) {
      console.log(JSON.stringify(authJwks({ masterSecret: a }), null, 2));
      return 0;
    }
    console.error("Usage: auto-cli auth <hash|verify|keygen|totp|jwks>");
    return 1;
  }

  const client = createAutoClientFromEnv();

  if (cmd === "tools") {
    const res = await client.callMcp("tools/list");
    const tools = (res.result as { tools?: { name: string; description: string }[] })?.tools ?? [];
    for (const tool of tools) console.log(`${tool.name.padEnd(28)} ${tool.description}`);
    return 0;
  }

  if (cmd === "mcp") {
    const decoder = new TextDecoder();
    const reader = Bun.stdin.stream().getReader();
    let buf = "";
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let nl = buf.indexOf("\n");
      while (nl !== -1) {
        const line = buf.slice(0, nl).trim();
        buf = buf.slice(nl + 1);
        nl = buf.indexOf("\n");
        if (!line) continue;
        try {
          const req = JSON.parse(line) as JsonRpcRequest;
          if (req.method === "initialize" || req.method === "ping" || req.method === "notifications/initialized") {
            const localReply = await handleMcpJsonRpc(req, async () => ({}));
            if (localReply) process.stdout.write(`${JSON.stringify(localReply)}\n`);
          } else {
            const remoteReply = await client.callMcp(req.method, req.params ?? {});
            const { jsonrpc: _jr, id: _id, ...rest } = remoteReply;
            process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: req.id ?? null, ...rest })}\n`);
          }
        } catch (err) {
          process.stdout.write(
            `${JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32700, message: err instanceof Error ? err.message : "Parse error" } })}\n`,
          );
        }
      }
    }
    return 0;
  }

  if (cmd === "workflows") {
    const [action = "list", arg1, arg2] = rest;
    if (action === "list") {
      console.log(JSON.stringify(await client.listAutomations(), null, 2));
      return 0;
    }
    if (action === "import" && arg1) {
      const raw = await Bun.file(arg1).json();
      const def = Array.isArray(raw.nodes) && raw.connections ? importLegacyN8nWorkflow(raw) : (raw.definition ?? raw);
      console.log(JSON.stringify(await client.saveAutomation({ name: raw.name ?? arg1, definition: def }), null, 2));
      return 0;
    }
    if (action === "run" && arg1) {
      const input = arg2 ? JSON.parse(arg2) : {};
      console.log(JSON.stringify(await client.runAutomation(arg1, input), null, 2));
      return 0;
    }
    console.error("Usage: auto-cli workflows <list|import <file.json>|run <id> [json]>");
    return 1;
  }

  if (cmd === "agent") {
    const llmCfg = llmConfigFromEnv();
    if (!llmCfg) {
      console.error(t(locale, "AUTO_LLM_URL is required for agent mode"));
      return 1;
    }
    const llm = openAiClient(llmCfg);
    const execute = async (name: string, args: Record<string, unknown>) => {
      const r = await client.mcpCallTool(name, args);
      return r.result ?? r.error;
    };
    if (rest.length) {
      const turn = await runAgentTurn(rest.join(" "), { llm, execute, locale });
      console.log(turn.reply);
      return 0;
    }
    let history: LlmMessage[] | undefined;
    process.stderr.write("> ");
    for await (const line of console) {
      if (line.trim()) {
        const turn = await runAgentTurn(line, { llm, execute, history, locale });
        history = turn.messages;
        console.log(turn.reply);
      }
      process.stderr.write("> ");
    }
    return 0;
  }

  console.error(`Unknown command: ${cmd}\nRun 'auto-cli help' for usage.`);
  return 1;
}

if (import.meta.main) {
  process.exitCode = await runCli(process.argv.slice(2));
}
