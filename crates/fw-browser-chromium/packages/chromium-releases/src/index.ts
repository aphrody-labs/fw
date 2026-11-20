import { Cache, cachedSource, liveSource, type Source } from "./cache.ts";
import { ChromiumDash } from "./dash.ts";
import { ChromeStatus } from "./status.ts";
import type { GetOptions } from "./http.ts";

export * from "./cache.ts";
export * from "./channels.ts";
export * from "./dash.ts";
export * from "./http.ts";
export * from "./pick.ts";
export * from "./status.ts";

export interface Clients {
  dash: ChromiumDash;
  status: ChromeStatus;
  source: Source;
  cache?: Cache;
}

/** Both clients over one source: sqlite-cached by default, `cache: false` for no storage, or a `Cache` you opened. */
export function createClients(options: { cache?: Cache | false; maxAgeMs?: number } & GetOptions = {}): Clients {
  const cache = options.cache === false ? undefined : (options.cache ?? new Cache());
  const source = cache ? cachedSource(cache, { maxAgeMs: options.maxAgeMs ?? 15 * 60_000, fetch: options.fetch, timeoutMs: options.timeoutMs, retries: options.retries }) : liveSource(options);
  return { dash: new ChromiumDash(source), status: new ChromeStatus(source), source, cache };
}
