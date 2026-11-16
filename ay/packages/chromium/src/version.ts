/** Which Chromium, and which CEF build, to embed for a release channel: live Chromium Dash and CEF data through @aphrody/chromium-releases. */
import { Cache, createClients, loadCefIndex, pickChromium, type CefPlatform, type FetchLike, type Selection } from "@aphrody/chromium-releases";

export type { Selection } from "@aphrody/chromium-releases";

export interface PickVersionOptions {
  /** `stable` (default), `extended`, `beta`, `dev` or `canary`. */
  channel?: string;
  /** CEF platform id (`windows64`, `linux64`, `macosarm64`, ...); default the host's. */
  cefPlatform?: CefPlatform;
  /** Sqlite cache to read through; default the shared user cache, `false` for none. */
  cache?: Cache | false;
  fetch?: FetchLike;
}

/**
 * The Chromium version a channel ships and the CEF build closest to it. `match` says how close: `exact` (same Chromium),
 * `milestone` (same major) or `behind` (CEF trails the channel by `lag` milestones: CEF stable was 156 while Chrome stable was 158).
 */
export async function pickVersion(options: PickVersionOptions = {}): Promise<Selection> {
  const clients = createClients({ cache: options.cache, fetch: options.fetch });
  try {
    return await pickChromium(clients.dash, () => loadCefIndex({ cache: clients.cache, fetch: options.fetch }), { channel: options.channel, cefPlatform: options.cefPlatform });
  } finally {
    if (options.cache === undefined) clients.cache?.close();
  }
}
