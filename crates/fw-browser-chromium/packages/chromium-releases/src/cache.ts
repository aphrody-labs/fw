import { Database } from "bun:sqlite";
import { mkdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { getJson, type GetOptions } from "./http.ts";

export interface Entry<T> {
  value: T;
  /** Epoch milliseconds when the upstream was read. */
  fetchedAt: number;
  /** True when the upstream failed and the last stored copy was served instead. */
  stale: boolean;
}

/** The CLI name from ay.toml (`[product].cli`), the directory name under the user cache. */
function cliName(): string {
  try {
    const toml = Bun.TOML.parse(readFileSync(join(import.meta.dir, "../../../ay.toml"), "utf8")) as { product?: { cli?: string } };
    return toml.product?.cli ?? "ay";
  } catch {
    return "ay";
  }
}

/** `AY_CHROMIUM_RELEASES_DB`, else `<user cache>/<cli>/chromium-releases.sqlite`. */
export function defaultDbPath(): string {
  if (process.env.AY_CHROMIUM_RELEASES_DB) return process.env.AY_CHROMIUM_RELEASES_DB;
  const base = process.env.XDG_CACHE_HOME ?? (process.platform === "win32" ? process.env.LOCALAPPDATA : undefined) ?? join(homedir(), ".cache");
  return join(base, cliName(), "chromium-releases.sqlite");
}

/** A sqlite-backed read-through cache of upstream JSON documents, keyed by URL. */
export class Cache {
  readonly db: Database;

  constructor(path: string = defaultDbPath()) {
    if (path !== ":memory:") mkdirSync(dirname(path), { recursive: true });
    this.db = new Database(path, { create: true });
    this.db.run("PRAGMA journal_mode = WAL");
    this.db.run("CREATE TABLE IF NOT EXISTS entries (key TEXT PRIMARY KEY, body TEXT NOT NULL, fetched_at INTEGER NOT NULL)");
  }

  read<T>(key: string): { value: T; fetchedAt: number } | undefined {
    const row = this.db.query<{ body: string; fetched_at: number }, [string]>("SELECT body, fetched_at FROM entries WHERE key = ?").get(key);
    return row ? { value: JSON.parse(row.body) as T, fetchedAt: row.fetched_at } : undefined;
  }

  write(key: string, value: unknown, now = Date.now()): void {
    this.db.query("INSERT INTO entries (key, body, fetched_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET body = ?2, fetched_at = ?3").run(key, JSON.stringify(value), now);
  }

  keys(): string[] {
    return this.db.query<{ key: string }, []>("SELECT key FROM entries ORDER BY key").all().map(r => r.key);
  }

  /** Delete every entry older than `maxAgeMs`; returns how many went. */
  prune(maxAgeMs: number, now = Date.now()): number {
    return this.db.query("DELETE FROM entries WHERE fetched_at < ?").run(now - maxAgeMs).changes;
  }

  close(): void {
    this.db.close();
  }
}

export interface Source {
  /** Fetch a JSON document; `maxAgeMs` is how old a cached copy may be (0 forces a refresh). */
  get<T>(url: string, maxAgeMs?: number): Promise<Entry<T>>;
}

/** Read-through source: a fresh copy comes from the cache, otherwise from the upstream, falling back to a stale copy when it fails. */
export function cachedSource(cache: Cache, defaults: { maxAgeMs: number } & GetOptions): Source {
  return {
    async get<T>(url: string, maxAgeMs = defaults.maxAgeMs): Promise<Entry<T>> {
      const hit = cache.read<T>(url);
      if (hit && Date.now() - hit.fetchedAt < maxAgeMs) return { ...hit, stale: false };
      try {
        const value = await getJson<T>(url, defaults);
        const fetchedAt = Date.now();
        cache.write(url, value, fetchedAt);
        return { value, fetchedAt, stale: false };
      } catch (e) {
        if (hit) return { ...hit, stale: true };
        throw e;
      }
    },
  };
}

/** A source with no storage: every call goes to the upstream. */
export function liveSource(options: GetOptions = {}): Source {
  return {
    async get<T>(url: string): Promise<Entry<T>> {
      return { value: await getJson<T>(url, options), fetchedAt: Date.now(), stale: false };
    },
  };
}
