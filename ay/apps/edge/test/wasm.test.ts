// The wasm service inside a real workerd: `wrangler dev --local` on a free port with the dev config of tools/edge-wasm.ts, then real
// HTTP requests. The test is skipped only when wrangler or workerd cannot start on this machine (the reason is printed).
import { afterAll, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { build } from "../../../tools/edge-wasm.ts";
import { root } from "../../../tools/corelib.ts";

let toolchain = "";
let built: ReturnType<typeof build> | undefined;
try {
  built = build();
} catch (e) {
  toolchain = `wasm toolchain unavailable: ${String(e).split("\n")[0]}`;
}
const index = built as ReturnType<typeof build>;
const out = join(root, "target/edge-wasm");

async function freePort(): Promise<number> {
  const s = Bun.serve({ port: 0, fetch: () => new Response("") });
  const port = s.port!;
  await s.stop(true);
  return port;
}

async function start(): Promise<{ base: string; stop: () => void } | string> {
  const wrangler = Bun.which("wrangler");
  const cmd = wrangler ? [wrangler] : ["bunx", "wrangler"];
  const port = await freePort();
  const proc = Bun.spawn([...cmd, "dev", "-c", join(out, "wrangler.jsonc"), "--local", "--port", String(port), "--ip", "127.0.0.1"], { cwd: root, stdout: "pipe", stderr: "pipe", env: { ...process.env, WRANGLER_SEND_METRICS: "false" } });
  let log = "";
  const pump = async (s: ReadableStream<Uint8Array>) => {
    for await (const chunk of s) log += new TextDecoder().decode(chunk);
  };
  void pump(proc.stdout);
  void pump(proc.stderr);
  const stop = () => {
    if (process.platform === "win32") Bun.spawnSync(["taskkill", "/PID", String(proc.pid), "/T", "/F"], { stdout: "ignore", stderr: "ignore" });
    else proc.kill();
  };
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 120; i++) {
    if (proc.exitCode !== null) return `wrangler dev exited ${proc.exitCode}: ${log.slice(-400)}`;
    try {
      if ((await fetch(`${base}/index.json`)).ok) return { base, stop };
    } catch {}
    await Bun.sleep(500);
  }
  stop();
  return `wrangler dev did not become ready: ${log.slice(-400)}`;
}

const server = toolchain || (await start());
const running = typeof server === "object";
if (!running) console.warn(`skipping the workerd tests: ${server}`);
afterAll(() => {
  if (running) server.stop();
});

describe.skipIf(!running)("wasm service in workerd", () => {
  const get = (path: string, init?: RequestInit) => fetch(`${(server as { base: string }).base}${path}`, init);

  test("index lists every package with its digest", async () => {
    const body = (await (await get("/index.json")).json()) as typeof index;
    expect(body.schema).toBe("ay.wasm.v1");
    expect(body.packages.map(p => p.name)).toEqual(index.packages.map(p => p.name));
    expect(body.packages[0]!.imports).toEqual([{ module: "ay_host", name: "console_put", kind: "function" }]);
  });

  test("the raw module is served with its digest as ETag, ranges and an immutable cache", async () => {
    const pkg = index.packages[0]!;
    const res = await get(`/${pkg.name}.wasm`);
    expect(res.headers.get("content-type")).toBe("application/wasm");
    expect(res.headers.get("etag")).toBe(`"${pkg.sha256}"`);
    expect(res.headers.get("cache-control")).toContain("immutable");
    const bytes = new Uint8Array(await res.arrayBuffer());
    expect(createHash("sha256").update(bytes).digest("hex")).toBe(pkg.sha256);
    expect(WebAssembly.validate(bytes)).toBe(true);
    expect((await get(`/${pkg.name}.wasm`, { headers: { "if-none-match": `"${pkg.sha256}"` } })).status).toBe(304);
    const part = await get(`/${pkg.name}.wasm`, { headers: { range: "bytes=0-7" } });
    expect(part.status).toBe(206);
    expect(part.headers.get("content-range")).toBe(`bytes 0-7/${pkg.size}`);
    expect([...new Uint8Array(await part.arrayBuffer())]).toEqual([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0]);
    expect((await get(`/${pkg.sha256}.wasm`)).status).toBe(200);
    expect((await get("/missing.wasm")).status).toBe(404);
  });

  test("exports of the instantiated corelib answer real calls", async () => {
    const call = async (path: string) => ((await (await get(path)).json()) as { result: number }).result;
    expect(await call("/invoke/ay_abi_version")).toBe(1);
    expect(await call("/invoke/ay_sum?words=1,2,3,4")).toBe(10);
    expect(await call("/invoke/ay_checksum?words=1,2,3,4")).toBe(0xe5bacf84);
    expect(await call("/invoke/ay_arg_count?nr=2")).toBe(3);
    expect(await call("/invoke/ay_rights_allow?have=5&need=4")).toBe(1);
    expect(await call("/invoke/ay_rights_allow?have=1&need=4")).toBe(0);
    expect(await call("/invoke/ay_first_free?bits=7")).toBe(3);
  });

  test("bad input is a 400, an absent export a 422, an unknown function a 404", async () => {
    expect((await get("/invoke/ay_sum?words=x")).status).toBe(400);
    expect((await get("/invoke/ay_sum?pkg=ay-corelib-component&words=1")).status).toBe(422);
    expect((await get("/invoke/nope")).status).toBe(404);
    expect((await get("/index.json", { method: "POST" })).status).toBe(405);
  });
});
