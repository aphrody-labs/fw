# fw MCP: recorded client sessions

Sessions with real clients against the `fw` built from this tree, recorded on 2026-11-21 with `tools/mcp-conformance/tee.ts` (stdio) and `tools/mcp-conformance/tee-http.ts` (HTTP), which copy every message in both directions. Lines are abridged to 230 characters (`...(N bytes)` marks the cut). The conformance numbers are in `mcp-conformance.md`.

| Client | Transport | Protocol version it spoke | Result |
|---|---|---|---|
| Claude Code 2.1.329 (`claude -p` with `--mcp-config`, tool call answered by the model) | stdio (`ssh fr1 …/fw mcp`) | 2026-07-28: `server/discover` probe, then `tools/list`, `tools/call` | `kernel: linux, arch: x86_64` |
| Claude Code 2.1.329 | Streamable HTTP (`fw serve`, through an SSH tunnel and `tee-http.ts`) | 2026-07-28 with `Mcp-Method`, `Mcp-Name` and `MCP-Protocol-Version` headers | `serde: ranked 20th overall of 3697 crates (score 90.43, version 1.0.229)` |
| Official inspector `@modelcontextprotocol/inspector` 2.16.0 (`--cli`) | stdio | 2025-11-25 (`initialize`) | tools listed, `linux_kernel_mcp` and `crates_info` answered |
| Official inspector 2.16.0 | Streamable HTTP | 2025-11-25 (`initialize`; its GET stream answered `405`, as the specification asks) | `wasm_kernel_mcp` answered |

The commands:

```sh
# Claude Code, stdio: the config names an ssh command that runs `bun tee.ts <log> -- fw mcp` on the build host
claude -p "Call the MCP tool mcp__fwfr1__linux_kernel_mcp with action probe …" \
  --mcp-config stdio.json --strict-mcp-config --allowedTools mcp__fwfr1__linux_kernel_mcp
# Claude Code, HTTP: {"mcpServers":{"fwhttp":{"type":"http","url":"http://127.0.0.1:43998/mcp"}}}
claude -p "Call the MCP tool mcp__fwhttp__crates_info with name serde …" --mcp-config http.json --strict-mcp-config
# Inspector
npx @modelcontextprotocol/inspector@2.16.0 --cli <wrapper around fw mcp> --method tools/call --tool-name linux_kernel_mcp --tool-arg action=probe
npx @modelcontextprotocol/inspector@2.16.0 --cli http://127.0.0.1:43998/mcp --transport http --method tools/call --tool-name wasm_kernel_mcp --tool-arg action=probe
```

## Transcripts

```text
### Inspector 2.16.0 -> fw mcp (stdio)
-> {"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"roots":{"listChanged":true},"extensions":{"io.modelcontextprotocol/tasks":{},"io.modelcontextprotocol/skills":{}}},"clientInfo":{"name":"inspector ...(280 bytes)
<- {"id":0,"jsonrpc":"2.0","result":{"capabilities":{"tools":{"listChanged":false}},"instructions":"fw: crate intelligence (crates_*), workspace files and search, host target probes, the package-manager scan and CLI tools such as  ...(332 bytes)
-> {"jsonrpc":"2.0","method":"notifications/initialized"}
-> {"method":"tools/list","jsonrpc":"2.0","id":1}
<- {"id":1,"jsonrpc":"2.0","result":{"tools":[{"annotations":{"destructiveHint":false,"idempotentHint":true,"openWorldHint":true,"readOnlyHint":true},"description":"Structural Tree-sitter AST code search using ast-grep","inputSche ...(9365 bytes)
-> {"method":"tools/call","params":{"name":"linux_kernel_mcp","arguments":{"action":"probe"}},"jsonrpc":"2.0","id":2}
<- {"id":2,"jsonrpc":"2.0","result":{"content":[{"text":"{\"inProcess\":true,\"latencyUs\":181,\"result\":{\"action\":\"probe\",\"arch\":\"x86_64\",\"capabilities\":[\"sched_ext:scx_lavd\",\"io_uring\",\"cgroups-v2\",\"namespaces\ ...(1299 bytes)
### Claude Code 2.1.329 -> fw mcp (stdio)
-> {"jsonrpc":"2.0","id":"server-discover-probe-1","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"claude-code","title":"Claude Cod ...(461 bytes)
<- {"id":"server-discover-probe-1","jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"cacheScope":"public","capabilities":{"tools":{"listChanged":false}},"instructions":"fw: c ...(494 bytes)
-> {"method":"tools/list","jsonrpc":"2.0","id":0,"params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"claude-code","title":"Claude Code","version":"2.1.329","descr ...(432 bytes)
<- {"id":0,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"cacheScope":"public","resultType":"complete","tools":[{"annotations":{"destructiveHint":false,"idempotentHint":tr ...(9505 bytes)
-> {"method":"tools/call","params":{"name":"linux_kernel_mcp","arguments":{"action":"probe"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"claude-code","title":"Claud ...(563 bytes)
<- {"id":1,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"content":[{"text":"{\"inProcess\":true,\"latencyUs\":245,\"result\":{\"action\":\"probe\",\"arch\":\"x86_64\",\"c ...(1402 bytes)
### Claude Code 2.1.329, then Inspector 2.16.0 -> fw serve (HTTP)
-> POST /mcp [accept: application/json, text/event-stream; content-type: application/json; host: 127.0.0.1:43998; mcp-method: server/discover; mcp-protocol-version: 2026-07-28] {"jsonrpc":"2.0","id":"server-discover-probe-1","meth ...(635 bytes)
<- 200 [application/json] {"id":"server-discover-probe-1","jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"cacheScope":"public","capabilities":{"tools":{"listChanged":false} ...(517 bytes)
-> POST /mcp [accept: application/json, text/event-stream; content-type: application/json; host: 127.0.0.1:43998; mcp-method: tools/list; mcp-protocol-version: 2026-07-28] {"method":"tools/list","jsonrpc":"2.0","id":0,"params":{"_ ...(601 bytes)
<- 200 [application/json] {"id":0,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"cacheScope":"public","resultType":"complete","tools":[{"annotations":{"destructiveHint":fa ...(9528 bytes)
-> POST /mcp [accept: application/json, text/event-stream; content-type: application/json; host: 127.0.0.1:43998; mcp-method: tools/call; mcp-name: crates_info; mcp-protocol-version: 2026-07-28] {"method":"tools/call","params":{"n ...(748 bytes)
<- 200 [application/json] {"id":1,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"content":[{"text":"{\"crate\":{\"categories\":[\"encoding\",\"no-std\",\"no-std::no-alloc\ ...(1893 bytes)

### tools/mcp-conformance client -> fw mcp (stdio), 2026-07-28
-> {"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"fw-mcp-conformance","version":"1"},"io.modelconte…(256 b
<- {"id":1,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"cacheScope":"public","capabilities":{"tools":{"listChanged":false}},"instructions":"fw: crate intelligence…(467 b
-> {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"fw-mcp-conformance","version":"1"},"io.modelcontextpro…(251 b
<- {"id":2,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"cacheScope":"public","resultType":"complete","tools":[{"annotations":{"destructiveHint":false,"idempotentH…(9502 
-> {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"linux_kernel_mcp","arguments":{"action":"probe"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name…(308 b
<- {"id":3,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"content":[{"text":"{\"inProcess\":true,\"latencyUs\":168,\"result\":{\"action\":\"probe\",\"arch\":\"x86_6…(1399 
-> {"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"crates_info","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"fw-mcp-conformance…(287 b
<- {"id":4,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fw","version":"4.2.0"}},"content":[{"text":"Invalid arguments for tool crates_info: Missing required property 'name'","type":"text"…(263 b
-> {"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"no_such_tool","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"fw-mcp-conformanc…(288 b
<- {"error":{"code":-32602,"message":"Unknown tool: no_such_tool"},"id":5,"jsonrpc":"2.0"}
```
