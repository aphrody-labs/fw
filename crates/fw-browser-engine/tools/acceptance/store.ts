// State of the acceptance tool. SQLite (bun:sqlite) is the record: runs, pages, the link index, benchmarks,
// upstream snapshots and the event log. Redis (Bun's client; Valkey speaks the same protocol) is the fast lane:
// a TTL cache shared by every process and host, and the channel that wakes the dashboard the instant a row lands.
// Every Redis call is best effort: without a server the tool is slower and the dashboard polls, nothing is lost.
import { RedisClient } from "bun";
import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

export type Event = { id: number; at: string; kind: string; data: any };
export type IndexRow = {
  crawl: string;
  url: string;
  host: string;
  parent: string | null;
  depth: number;
  state: "pending" | "done" | "leaf" | "error";
  status: number | null;
  etag: string | null;
  modified: string | null;
  links: string | null;
  fetched: string | null;
};
export type RunRow = {
  id: string;
  started: string;
  finished: string | null;
  ok: number | null;
  host: string;
  meta: any;
};

const prefix = "acceptance";
const schema = `
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 10000;
CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, started TEXT NOT NULL, finished TEXT, ok INTEGER, host TEXT NOT NULL, meta TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS run_engines (run TEXT NOT NULL, engine TEXT NOT NULL, info TEXT NOT NULL, PRIMARY KEY (run, engine));
CREATE TABLE IF NOT EXISTS pages (run TEXT NOT NULL, engine TEXT NOT NULL, url TEXT NOT NULL, verdict TEXT NOT NULL, at TEXT NOT NULL,
  result TEXT NOT NULL, PRIMARY KEY (run, engine, url));
CREATE TABLE IF NOT EXISTS index_pages (crawl TEXT NOT NULL, url TEXT NOT NULL, host TEXT NOT NULL, parent TEXT, depth INTEGER NOT NULL,
  state TEXT NOT NULL, status INTEGER, etag TEXT, modified TEXT, links TEXT, fetched TEXT, PRIMARY KEY (crawl, url));
CREATE INDEX IF NOT EXISTS index_pages_url ON index_pages (url, fetched);
CREATE TABLE IF NOT EXISTS bench (id INTEGER PRIMARY KEY, at TEXT NOT NULL, host TEXT NOT NULL, suite TEXT NOT NULL, engine TEXT NOT NULL,
  version TEXT, target TEXT NOT NULL, result TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS upstreams (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, at TEXT NOT NULL, snapshot TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS upstreams_name ON upstreams (name, id);
CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL, kind TEXT NOT NULL, data TEXT NOT NULL);
`;

export class Store {
  readonly db: Database;
  #redis: RedisClient | null;
  readonly redis: string | null; // the server in use, for reports

  private constructor(db: Database, redis: RedisClient | null, url: string | null) {
    [this.db, this.#redis, this.redis] = [db, redis, url];
  }

  /** `redis`: a URL, or undefined for $REDIS_URL / $VALKEY_URL / localhost:6379, or false for none. */
  static async open(path: string, redis?: string | false): Promise<Store> {
    if (path !== ":memory:") mkdirSync(dirname(path), { recursive: true });
    const db = new Database(path, { create: true, strict: true });
    db.exec(schema);
    if (redis === false) return new Store(db, null, null);
    const url = redis ?? process.env.REDIS_URL ?? process.env.VALKEY_URL ?? "redis://127.0.0.1:6379";
    const client = new RedisClient(url, { connectionTimeout: 1500, autoReconnect: true, enableOfflineQueue: false });
    try {
      await client.connect();
      await client.ping();
      return new Store(db, client, url.replace(/\/\/[^@/]*@/, "//")); // never report credentials
    } catch {
      client.close();
      return new Store(db, null, null);
    }
  }

  close() {
    this.#redis?.close();
    this.db.close();
  }

  // ---- hot cache ----

  async cached<T>(key: string): Promise<T | null> {
    const text = await this.#redis?.get(`${prefix}:${key}`).catch(() => null);
    return text ? (JSON.parse(text) as T) : null;
  }

  async cache(key: string, value: unknown, ttl_s: number) {
    await this.#redis
      ?.set(`${prefix}:${key}`, JSON.stringify(value), "EX", Math.max(1, Math.round(ttl_s)))
      .catch(() => {});
  }

  // ---- events: the log is durable, the publish is only the wake-up ----

  emit(kind: string, data: unknown): number {
    const at = new Date().toISOString();
    const { lastInsertRowid } = this.db.run("INSERT INTO events (at, kind, data) VALUES (?, ?, ?)", [
      at,
      kind,
      JSON.stringify(data),
    ]);
    this.#redis?.publish(prefix, String(lastInsertRowid)).catch(() => {});
    return Number(lastInsertRowid);
  }

  eventsAfter(id: number, limit = 1000): Event[] {
    const rows = this.db
      .query("SELECT id, at, kind, data FROM events WHERE id > ? ORDER BY id LIMIT ?")
      .all(id, limit) as any[];
    return rows.map((r) => ({ ...r, data: JSON.parse(r.data) }));
  }

  lastEvent(): number {
    return (this.db.query("SELECT COALESCE(MAX(id), 0) AS id FROM events").get() as { id: number }).id;
  }

  /** Calls `wake` when another process may have written: at once through Redis, and on a slow tick regardless. */
  async watch(wake: () => void, tick_ms = 1000): Promise<() => void> {
    const timer = setInterval(wake, tick_ms);
    const listener = await this.#redis?.duplicate().catch(() => null);
    await listener?.subscribe(prefix, wake).catch(() => {});
    return () => {
      clearInterval(timer);
      listener?.close();
    };
  }

  // ---- runs ----

  unfinishedRun(host: string): RunRow | null {
    const row = this.db
      .query("SELECT * FROM runs WHERE finished IS NULL AND host = ? ORDER BY started DESC")
      .get(host) as any;
    return row ? { ...row, meta: JSON.parse(row.meta) } : null;
  }

  /** Starts a run; whatever this host left unfinished is closed as failed. */
  createRun(id: string, host: string, meta: object): RunRow {
    const run = { id, started: new Date().toISOString(), finished: null, ok: null, host, meta };
    this.db.run("UPDATE runs SET finished = ?, ok = 0 WHERE finished IS NULL AND host = ?", [run.started, host]);
    this.db.run("INSERT INTO runs (id, started, host, meta) VALUES (?, ?, ?, ?)", [
      id,
      run.started,
      host,
      JSON.stringify(meta),
    ]);
    return run;
  }

  finishRun(id: string, ok: boolean) {
    this.db.run("UPDATE runs SET finished = ?, ok = ? WHERE id = ?", [new Date().toISOString(), ok ? 1 : 0, id]);
  }

  runs(limit = 30): RunRow[] {
    const rows = this.db.query("SELECT * FROM runs ORDER BY started DESC LIMIT ?").all(limit) as any[];
    return rows.map((r) => ({ ...r, meta: JSON.parse(r.meta) }));
  }

  engine<T>(run: string, engine: string): T | null {
    const row = this.db.query("SELECT info FROM run_engines WHERE run = ? AND engine = ?").get(run, engine) as {
      info: string;
    } | null;
    return row ? (JSON.parse(row.info) as T) : null;
  }

  engines<T>(run: string): T[] {
    const rows = this.db.query("SELECT info FROM run_engines WHERE run = ? ORDER BY rowid").all(run) as {
      info: string;
    }[];
    return rows.map((r) => JSON.parse(r.info) as T);
  }

  saveEngine(run: string, engine: string, info: object) {
    this.db.run(
      "INSERT INTO run_engines (run, engine, info) VALUES (?, ?, ?) ON CONFLICT DO UPDATE SET info = excluded.info",
      [run, engine, JSON.stringify(info)],
    );
  }

  pages<T>(run: string, engine: string): Map<string, T> {
    const rows = this.db.query("SELECT url, result FROM pages WHERE run = ? AND engine = ?").all(run, engine) as any[];
    return new Map(rows.map((r) => [r.url, JSON.parse(r.result) as T]));
  }

  savePage(run: string, engine: string, page: { url: string; verdict: string }) {
    this.db.run(
      "INSERT INTO pages (run, engine, url, verdict, at, result) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT DO UPDATE SET verdict = excluded.verdict, at = excluded.at, result = excluded.result",
      [run, engine, page.url, page.verdict, new Date().toISOString(), JSON.stringify(page)],
    );
  }

  dropPages(run: string, engine: string) {
    this.db.run("DELETE FROM pages WHERE run = ? AND engine = ?", [run, engine]);
  }

  // ---- link index ----

  /** The crawl to work on: the unfinished one, else the last one while it is younger than `ttl_s`, else a new one. */
  openCrawl(
    id: string,
    roots: string[],
    ttl_s: number,
    fresh: boolean,
  ): { crawl: string; resumed: boolean; reused: boolean } {
    const last = this.db
      .query(
        "SELECT crawl, MAX(fetched) AS fetched, SUM(state = 'pending') AS pending FROM index_pages GROUP BY crawl ORDER BY crawl DESC",
      )
      .get() as { crawl: string; fetched: string | null; pending: number } | null;
    if (last && !fresh) {
      if (last.pending) return { crawl: last.crawl, resumed: true, reused: false };
      if (last.fetched && Date.now() - Date.parse(last.fetched) < ttl_s * 1000)
        return { crawl: last.crawl, resumed: false, reused: true };
    }
    for (const url of roots)
      this.addIndex({ crawl: id, url, host: new URL(url).host, parent: null, depth: 0, state: "pending" });
    return { crawl: id, resumed: false, reused: false };
  }

  addIndex(row: Pick<IndexRow, "crawl" | "url" | "host" | "parent" | "depth" | "state">) {
    this.db.run(
      "INSERT OR IGNORE INTO index_pages (crawl, url, host, parent, depth, state) VALUES (?, ?, ?, ?, ?, ?)",
      [row.crawl, row.url, row.host, row.parent, row.depth, row.state],
    );
  }

  pendingIndex(crawl: string, limit: number): IndexRow[] {
    return this.db
      .query("SELECT * FROM index_pages WHERE crawl = ? AND state = 'pending' ORDER BY depth, url LIMIT ?")
      .all(crawl, limit) as IndexRow[];
  }

  fetchedPerHost(crawl: string): Map<string, number> {
    const rows = this.db
      .query("SELECT host, COUNT(*) AS n FROM index_pages WHERE crawl = ? AND state IN ('done', 'error') GROUP BY host")
      .all(crawl) as any[];
    return new Map(rows.map((r) => [r.host, r.n]));
  }

  /** Validators and links of the last successful read of a URL, whatever the crawl. */
  lastFetch(url: string): Pick<IndexRow, "etag" | "modified" | "links" | "fetched"> | null {
    return this.db
      .query(
        "SELECT etag, modified, links, fetched FROM index_pages WHERE url = ? AND state = 'done' AND links IS NOT NULL ORDER BY fetched DESC",
      )
      .get(url) as any;
  }

  saveIndex(crawl: string, url: string, row: Partial<IndexRow> & { state: IndexRow["state"] }) {
    this.db.run(
      "UPDATE index_pages SET state = ?, status = ?, etag = ?, modified = ?, links = ?, fetched = ? WHERE crawl = ? AND url = ?",
      [
        row.state,
        row.status ?? null,
        row.etag ?? null,
        row.modified ?? null,
        row.links ?? null,
        "fetched" in row ? (row.fetched ?? null) : new Date().toISOString(),
        crawl,
        url,
      ],
    );
  }

  index(crawl: string): IndexRow[] {
    return this.db.query("SELECT * FROM index_pages WHERE crawl = ? ORDER BY depth, url").all(crawl) as IndexRow[];
  }

  // ---- benchmarks and upstreams ----

  saveBench(row: {
    host: string;
    suite: string;
    engine: string;
    version: string | null;
    target: string;
    result: object;
  }) {
    this.db.run("INSERT INTO bench (at, host, suite, engine, version, target, result) VALUES (?, ?, ?, ?, ?, ?, ?)", [
      new Date().toISOString(),
      row.host,
      row.suite,
      row.engine,
      row.version,
      row.target,
      JSON.stringify(row.result),
    ]);
  }

  bench(limit = 400): any[] {
    const rows = this.db.query("SELECT * FROM bench ORDER BY id DESC LIMIT ?").all(limit) as any[];
    return rows.map((r) => ({ ...r, result: JSON.parse(r.result) }));
  }

  /** A snapshot is stored when something moved since the last one of that upstream; returns whether it did. */
  saveUpstream(name: string, snapshot: object): boolean {
    const text = JSON.stringify(snapshot);
    const last = this.db.query("SELECT snapshot FROM upstreams WHERE name = ? ORDER BY id DESC").get(name) as {
      snapshot: string;
    } | null;
    if (last?.snapshot === text) return false;
    this.db.run("INSERT INTO upstreams (name, at, snapshot) VALUES (?, ?, ?)", [name, new Date().toISOString(), text]);
    return true;
  }

  upstreams(history = 30): { name: string; at: string; snapshot: any; history: { at: string; snapshot: any }[] }[] {
    const names = this.db.query("SELECT DISTINCT name FROM upstreams ORDER BY name").all() as { name: string }[];
    return names.map(({ name }) => {
      const rows = this.db
        .query("SELECT at, snapshot FROM upstreams WHERE name = ? ORDER BY id DESC LIMIT ?")
        .all(name, history) as any[];
      const parsed = rows.map((r) => ({ at: r.at as string, snapshot: JSON.parse(r.snapshot) }));
      return { name, ...parsed[0], history: parsed };
    });
  }
}
