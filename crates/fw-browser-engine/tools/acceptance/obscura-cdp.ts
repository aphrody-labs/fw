// Playwright connectOverCDP against `obscura serve` (lite engine): navigate, evaluate, no paint.
// usage: bun tools/acceptance/obscura-cdp.ts <obscura-binary> <url>
import { chromium } from "playwright-core";
import { spawn } from "bun";
const [bin, url] = Bun.argv.slice(2);
const port = 9600 + Math.floor(Math.random() * 300);
const proc = spawn([bin, "serve", "--port", String(port)], { stdout: "ignore", stderr: "ignore" });
let browser;
for (let i = 0; i < 50 && !browser; i++) {
  try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`); } catch { await Bun.sleep(200); }
}
if (!browser) throw new Error("no CDP endpoint");
const t0 = performance.now();
const ctx = browser.contexts()[0] ?? (await browser.newContext());
const page = ctx.pages()[0] ?? (await ctx.newPage());
await page.goto(url, { waitUntil: "load" });
const out = { title: await page.title(), links: await page.evaluate(() => document.links.length), ms: Math.round(performance.now() - t0) };
console.log(JSON.stringify(out));
await browser.close();
proc.kill();
