import { existsSync } from "node:fs";
import { dirname, join } from "node:path";

export interface ScreenshotOptions {
  /** Viewport width in CSS pixels. Default 1280. */
  width?: number;
  /** Viewport height in CSS pixels. Default 720. */
  height?: number;
  /** Milliseconds to wait after the load event before painting. Default 500. */
  settleMs?: number;
  /** Milliseconds before the call fails. Default 30000. */
  timeoutMs?: number;
}

export interface EvaluateOptions {
  /** Milliseconds to wait after the load event before evaluating. Default 0. */
  settleMs?: number;
  timeoutMs?: number;
}

export interface ScreenshotResult {
  path: string;
  width: number;
  height: number;
}

export interface Native {
  configure(config: { cefDir?: string; helper?: string; cacheDir?: string }): void;
  cefVersion(): string;
  shutdown(): void;
  screenshot(url: string, out: string, width?: number, height?: number, settleMs?: number, timeoutMs?: number): Promise<ScreenshotResult>;
  evaluate(url: string, expression: string, settleMs?: number, timeoutMs?: number): Promise<string>;
}

let native: Native | undefined;

/** Where the addon and the helper executable are: AY_CHROMIUM_HOME, else packages/chromium/native. */
function home(): string {
  return process.env.AY_CHROMIUM_HOME ?? join(import.meta.dir, "..", "native");
}

function load(): Native {
  if (native) return native;
  const dir = home();
  const addon = process.env.AY_CHROMIUM_ADDON ?? join(dir, "ay_chromium.node");
  if (!existsSync(addon)) {
    throw new Error(`ay_chromium addon not found at ${addon}; run \`bun run build\` in packages/chromium or set AY_CHROMIUM_HOME`);
  }
  const module = { exports: {} as Native };
  process.dlopen(module, addon);
  const helper = process.env.AY_CHROMIUM_HELPER ?? join(dirname(addon), "ay-chromium");
  module.exports.configure({
    helper: existsSync(helper) ? helper : undefined,
    cefDir: process.env.AY_CHROMIUM_CEF_DIR,
    cacheDir: process.env.AY_CHROMIUM_CACHE_DIR,
  });
  native = module.exports;
  return native;
}

/** Open `url` off-screen, wait for it to load and write a PNG of the viewport to `out`. */
export function screenshot(url: string, out: string, options: ScreenshotOptions = {}): Promise<ScreenshotResult> {
  return load().screenshot(url, out, options.width, options.height, options.settleMs, options.timeoutMs);
}

/** Open `url`, wait for it to load and evaluate `expression` in the page; promises are awaited and the value comes back as parsed JSON. */
export async function evaluate<T = unknown>(url: string, expression: string, options: EvaluateOptions = {}): Promise<T> {
  const json = await load().evaluate(url, expression, options.settleMs, options.timeoutMs);
  return JSON.parse(json) as T;
}

/** The CEF build the addon was compiled against. */
export function cefVersion(): string {
  return load().cefVersion();
}

/** Close the browsers and shut CEF down. CEF cannot be started again in this process. */
export function shutdown(): void {
  if (native) native.shutdown();
}
