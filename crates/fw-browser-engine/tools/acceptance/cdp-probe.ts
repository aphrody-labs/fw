// Drives a Chromium-family binary over raw CDP (no Playwright dependency) and reports what the Dioxus
// web corpus needs: WASM, WebGPU adapter, a rendered screenshot, a page evaluate.
// usage: bun tools/acceptance/cdp-probe.ts <browser-binary> <url> <out.png> [--swiftshader]
import { spawn } from "bun";

const [bin, url, out, ...flags] = Bun.argv.slice(2);
const gpu = flags.includes("--swiftshader")
  ? ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=swiftshader", "--use-webgpu-adapter=swiftshader", "--disable-vulkan-surface"]
  : ["--enable-unsafe-webgpu"];
const proc = spawn([bin, "--headless", "--no-sandbox", "--remote-debugging-port=0", ...gpu, "about:blank"],
  { stderr: "pipe", stdout: "ignore" });

const wsUrl: string = await (async () => {
  const dec = new TextDecoder();
  let buf = "";
  for await (const chunk of proc.stderr) {
    buf += dec.decode(chunk);
    const m = buf.match(/DevTools listening on (ws:\/\/\S+)/);
    if (m) return m[1];
  }
  throw new Error("no DevTools endpoint: " + buf.slice(-300));
})();

const ws = new WebSocket(wsUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const pending = new Map<number, (v: any) => void>();
const events: any[] = [];
ws.onmessage = (e) => {
  const m = JSON.parse(String(e.data));
  if (m.id && pending.has(m.id)) pending.get(m.id)!(m);
  else events.push(m);
};
const send = (method: string, params: any = {}, sessionId?: string) =>
  new Promise<any>((res) => {
    const i = ++id;
    pending.set(i, res);
    ws.send(JSON.stringify({ id: i, method, params, sessionId }));
  });

const t0 = performance.now();
const { result: tgt } = await send("Target.createTarget", { url: "about:blank" });
const { result: att } = await send("Target.attachToTarget", { targetId: tgt.targetId, flatten: true });
const s = att.sessionId;
await send("Page.enable", {}, s);
await send("Page.navigate", { url }, s);
for (let i = 0; i < 300 && !events.some((e) => e.method === "Page.loadEventFired" && e.sessionId === s); i++) await Bun.sleep(100);
await Bun.sleep(3000); // let a wasm app boot

const ev = async (expression: string) =>
  (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }, s)).result?.result?.value;
const report = {
  version: (await send("Browser.getVersion")).result.product,
  title: await ev("document.title"),
  wasm: await ev("typeof WebAssembly === 'object' && WebAssembly.validate(new Uint8Array([0,97,115,109,1,0,0,0]))"),
  webgpu: await ev("(async()=>{ if(!navigator.gpu) return 'no navigator.gpu'; const a = await navigator.gpu.requestAdapter(); return a ? 'adapter ok' : 'no adapter'; })()"),
  canvases: await ev("document.querySelectorAll('canvas').length"),
  elements: await ev("document.getElementsByTagName('*').length"),
  load_ms: Math.round(performance.now() - t0),
};
const shot = (await send("Page.captureScreenshot", { format: "png" }, s)).result.data;
await Bun.write(out, Buffer.from(shot, "base64"));
console.log(JSON.stringify({ ...report, screenshot_bytes: Buffer.from(shot, "base64").length }));
await send("Browser.close");
proc.kill();
