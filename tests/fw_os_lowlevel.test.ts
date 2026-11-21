import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import worker from "../cloudflare/worker.ts";
import { brainIndexRepo, brainSearch, FwProvider } from "../src/ai_stack.ts";
import { createFwApiHandler } from "../src/api.ts";
import { verifyLowLevelToolchains } from "../src/lowlevel.ts";
import { isNtfsReservedName, toNtfsSafePath } from "../src/ntfs.ts";
import { auditDeduplication } from "../src/plugin_sync.ts";
import {
  astQueryWithAbi,
  astSymbolsWithAbi,
  cryptoDigestWithAbi,
  disasmX64WithAbi,
  findAbiLibrary,
  gitInspectWithAbi,
  identifyFileWithAbi,
} from "../src/targets.ts";
import { renderFwTui } from "../src/tui.ts";
import { vfsList, vfsRead, vfsSnapshot, vfsSnapshots, vfsWrite } from "../src/vfs.ts";

const ROOT = process.cwd();
const IS_WIN = process.platform === "win32";

describe("polyglot OS, C ABI, NTFS, VFS, AI protocols and low-level LTO/AOT", () => {
  test("Fat-LTO ABI v4 exposes Tree-sitter symbols/query, craft-sniff, iced-x86 disasm, Rust crypto, and Git", () => {
    const syms = astSymbolsWithAbi(ROOT, "rust", join(ROOT, "native", "fw-abi", "src", "gguf.rs")) as {
      ok: boolean;
      symbolCount: number;
    };
    expect(syms.ok).toBe(true);
    expect(syms.symbolCount).toBeGreaterThan(3);

    const q = astQueryWithAbi(
      ROOT,
      "go",
      join(ROOT, "core", "go", "fw-go", "main.go"),
      "(function_declaration name: (identifier) @fn)",
    ) as { ok: boolean; matchCount: number };
    expect(q.ok).toBe(true);
    expect(q.matchCount).toBeGreaterThanOrEqual(3);

    const id = identifyFileWithAbi(ROOT, join(ROOT, "fw.targets.json")) as { ok: boolean; bytes: number };
    expect(id.ok).toBe(true);
    expect(id.bytes).toBeGreaterThan(100);

    const dig = cryptoDigestWithAbi(ROOT, "fw") as { ok: boolean; sha256: string; blake3: string };
    expect(dig.ok).toBe(true);
    expect(dig.sha256.length).toBe(64);
    expect(dig.blake3.length).toBe(64);

    const git = gitInspectWithAbi(ROOT, ROOT) as { ok: boolean; branch: string | null };
    expect(git.ok).toBe(true);
    // The branch the checkout is on (main, a release/<M> branch, or null when detached), read with git itself.
    const head = Bun.spawnSync(["git", "-C", ROOT, "symbolic-ref", "--quiet", "--short", "HEAD"]).stdout.toString().trim();
    expect(git.branch).toBe(head || null);

    if (IS_WIN) {
      const abiBin = findAbiLibrary(ROOT)!;
      const dis = disasmX64WithAbi(ROOT, abiBin, 8) as { ok: boolean; count: number; bitness: number };
      expect(dis.ok).toBe(true);
      expect(dis.bitness).toBe(64);
      expect(dis.count).toBeGreaterThan(0);
    }
  });

  test("NTFS safety helpers guard long paths, reserved device names, and plugin junctions", () => {
    expect(isNtfsReservedName("CON")).toBe(true);
    expect(isNtfsReservedName("nul.txt")).toBe(true);
    expect(isNtfsReservedName("fw.ts")).toBe(false);
    const longPath = join(ROOT, "a".repeat(120), "b".repeat(120), "file.txt");
    const safe = toNtfsSafePath(longPath);
    if (IS_WIN) {
      expect(safe.startsWith("\\\\?\\")).toBe(true);
    }
    const dedupe = auditDeduplication(ROOT);
    expect(dedupe.overlapWithAphrody).toEqual([]);
    expect(dedupe.ntfsJunctionsFound).toEqual([]);
  });

  test("VFS OS supports CAS write/read/ls/snapshot and AI Brain FTS5 indexes and searches", () => {
    const w = vfsWrite(ROOT, "/overlay/smoke/hello.txt", "fw-vfs-ok");
    expect(w.ok).toBe(true);
    const r = vfsRead(ROOT, "/overlay/smoke/hello.txt");
    expect(r.ok).toBe(true);
    expect(r.content).toBe("fw-vfs-ok");
    const ls = vfsList(ROOT, "/overlay/smoke");
    expect(ls.entries.some((e) => e.path === "/overlay/smoke/hello.txt")).toBe(true);

    const snap = vfsSnapshot(ROOT, "test-snap");
    expect(snap.ok).toBe(true);
    const snaps = vfsSnapshots(ROOT);
    expect(snaps.snapshots.some((s) => s.id === "test-snap")).toBe(true);

    const idx = brainIndexRepo(ROOT);
    expect(idx.indexed).toBeGreaterThanOrEqual(5);
    const hits = brainSearch(ROOT, "fw");
    expect(hits.count).toBeGreaterThan(0);
  });

  test("OpenAI, OpenAPI 3.1, A2A, ACP, MCP, TUI, Web UI, Provider, and Cloudflare Worker all respond validly", async () => {
    const handler = createFwApiHandler(ROOT);
    const openapi = (await (await handler(new Request("http://127.0.0.1:4379/openapi.json"))).json()) as { openapi: string };
    expect(openapi.openapi).toBe("3.1.0");

    const card = (await (await handler(new Request("http://127.0.0.1:4379/.well-known/agent.json"))).json()) as { name: string };
    expect(card.name).toBe("Fw");

    const chat = (await (
      await handler(
        new Request("http://127.0.0.1:4379/v1/chat/completions", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ model: "fw-local", messages: [{ role: "user", content: "fw" }] }),
        }),
      )
    ).json()) as { object: string; choices: Array<{ message: { content: string } }> };
    expect(chat.object).toBe("chat.completion");
    expect(chat.choices[0].message.content).toContain("fw");

    const acp = (await (
      await handler(
        new Request("http://127.0.0.1:4379/acp", {
          method: "POST",
          body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize" }),
        }),
      )
    ).json()) as { result: { protocolVersion: string } };
    expect(acp.result.protocolVersion).toBe("1.33.0");

    const a2a = (await (
      await handler(
        new Request("http://127.0.0.1:4379/a2a", {
          method: "POST",
          body: JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tasks/send", params: { prompt: "audit" } }),
        }),
      )
    ).json()) as { result: { status: { state: string } } };
    expect(a2a.result.status.state).toBe("completed");

    const html = await (await handler(new Request("http://127.0.0.1:4379/"))).text();
    expect(html).toContain("<title>fw</title>");
    expect(html).toContain("/app.js");

    const tui = renderFwTui(ROOT);
    expect(tui).toContain("FW OS TUI");

    const prov = new FwProvider(ROOT);
    expect(prov.listModels().data.length).toBeGreaterThan(3);

    const cfHealth = (await (await worker.fetch(new Request("https://fw.example/health"), {})).json()) as { ok: boolean };
    expect(cfHealth.ok).toBe(true);
    // Without a Workers AI binding the edge refuses chat explicitly (tests/fw_cloudflare.test.ts covers the live path).
    const cfChat = await worker.fetch(
      new Request("https://fw.example/v1/chat/completions", {
        method: "POST",
        body: JSON.stringify({ messages: [{ role: "user", content: "edge-ping" }] }),
      }),
      {},
    );
    expect(cfChat.status).toBe(503);
  });

  test("low-level cross-language & cross-architecture builds execute live (C+Asm, Wasm32, CUDA PTX, C++, WGSL, Go, Java)", async () => {
    const res = await verifyLowLevelToolchains(ROOT);
    expect(res.ok).toBe(true);
    expect(res.matrixCount).toBeGreaterThanOrEqual(12);
    const byName = new Map(res.steps.map((s) => [s.name, s]));
    expect(byName.get("webgpu-wgsl")?.ok).toBe(true);
    if (byName.has("c+x86_64-asm")) expect(byName.get("c+x86_64-asm")?.ok).toBe(true);
    if (byName.has("webassembly-wasm32")) expect(byName.get("webassembly-wasm32")?.ok).toBe(true);
    if (byName.has("cuda-nvptx64-ptx")) expect(byName.get("cuda-nvptx64-ptx")?.ok).toBe(true);
    if (byName.has("go-test")) expect(byName.get("go-test")?.ok).toBe(true);
    if (byName.has("java-ghidra-bridge")) expect(byName.get("java-ghidra-bridge")?.ok).toBe(true);
    // A cold Go build cache compiles the standard library inside `go test` (12 s on a 12-core host, measured with an empty GOCACHE); warm it takes 0.3 s.
  }, 120_000);
});
