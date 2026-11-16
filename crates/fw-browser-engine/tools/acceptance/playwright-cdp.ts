// Playwright over CDP against any Chromium-family binary (Chrome for Testing today, our headless_shell next).
// usage: bun tools/acceptance/playwright-cdp.ts <browser-binary> <url> <out.png>
// deps: bun add playwright-core (scratch install; the repo itself stays dependency-free)
import { chromium } from "playwright-core";
import { spawn } from "bun";

const [bin, url, out] = Bun.argv.slice(2);
const port = 9400 + Math.floor(Math.random() * 400);
const proc = spawn([bin, "--headless", "--no-sandbox", `--remote-debugging-port=${port}`, "--enable-unsafe-webgpu",
  "--use-angle=swiftshader", "--use-webgpu-adapter=swiftshader", "about:blank"], { stdout: "ignore", stderr: "ignore" });
let browser;
for (let i = 0; i < 50 && !browser; i++) {
  try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`); } catch { await Bun.sleep(200); }
}
if (!browser) throw new Error("no CDP endpoint");
const t0 = performance.now();
const page = browser.contexts()[0]?.pages()[0] ?? (await (browser.contexts()[0] ?? (await browser.newContext())).newPage());
await page.goto(url, { waitUntil: "load" });
const title = await page.title();
const sum = await page.evaluate(() => ({ links: document.links.length, ua: navigator.userAgent, wasm: typeof WebAssembly }));
await page.screenshot({ path: out });
console.log(JSON.stringify({ title, ...sum, ms: Math.round(performance.now() - t0), png: Bun.file(out).size }));
await browser.close();
proc.kill();
