// bun test testing/acceptance            (SQLite only; with REDIS_URL set the cache and wake-up paths run against that server too)
import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import {
  classify,
  isTransient,
  normalize,
  parity,
  spread,
  judge,
  outliers,
  readHyperfine,
  manifest,
  type PageResult,
} from "../acceptance.ts";
import { parseHttpsRecord } from "./net.ts";
import { Store } from "./store.ts";
import { merge } from "./local.ts";
import { rmrf, tmpdir } from "./sys.ts";

const open = () => Store.open(":memory:", Bun.env.REDIS_URL ?? false);

test("events survive in order and are replayed after an id", async () => {
  const store = await open();
  const ids = [store.emit("page", { n: 1 }), store.emit("page", { n: 2 }), store.emit("run", { n: 3 })];
  expect(ids).toEqual([1, 2, 3]);
  expect(store.eventsAfter(1).map((e) => e.data.n)).toEqual([2, 3]);
  expect(store.lastEvent()).toBe(3);
  store.close();
});

test("the index frontier is persisted and a second open resumes it", async () => {
  const store = await open();
  const first = store.openCrawl("c1", ["https://a.test/"], 3600, false);
  expect(first).toEqual({ crawl: "c1", resumed: false, reused: false });
  expect(store.pendingIndex("c1", 10).map((r) => r.url)).toEqual(["https://a.test/"]);
  expect(store.openCrawl("c2", ["https://a.test/"], 3600, false)).toEqual({
    crawl: "c1",
    resumed: true,
    reused: false,
  });
  store.saveIndex("c1", "https://a.test/", {
    state: "done",
    status: 200,
    etag: '"x"',
    links: JSON.stringify(["https://b.test/"]),
  });
  expect(store.openCrawl("c3", ["https://a.test/"], 3600, false)).toEqual({
    crawl: "c1",
    resumed: false,
    reused: true,
  });
  expect(store.openCrawl("c4", ["https://a.test/"], 3600, true).crawl).toBe("c4");
  expect(store.lastFetch("https://a.test/")?.etag).toBe('"x"');
  store.close();
});

test("a run resumes: pages already stored are kept, an unfinished run is closed by the next one", async () => {
  const store = await open();
  const run = store.createRun("r1", "h", { corpus: [] });
  store.savePage(run.id, "blink", { url: "https://a.test/", verdict: "pass" });
  expect(store.unfinishedRun("h")?.id).toBe("r1");
  expect([...store.pages(run.id, "blink").keys()]).toEqual(["https://a.test/"]);
  store.createRun("r2", "h", {});
  expect(store.runs().find((r) => r.id === "r1")).toMatchObject({ ok: 0 });
  store.close();
});

test("an upstream snapshot is stored only when it moved", async () => {
  const store = await open();
  expect(store.saveUpstream("servo", { head: "a" })).toBe(true);
  expect(store.saveUpstream("servo", { head: "a" })).toBe(false);
  expect(store.saveUpstream("servo", { head: "b" })).toBe(true);
  expect(store.upstreams()[0].snapshot).toEqual({ head: "b" });
  store.close();
});

test.skipIf(!Bun.env.REDIS_URL)("the hot cache round-trips and the wake-up reaches a watcher", async () => {
  const store = await open();
  expect(store.redis).not.toBeNull();
  await store.cache("k", { v: 1 }, 5);
  expect(await store.cached("k")).toEqual({ v: 1 });
  let woke = 0;
  const stop = await store.watch(() => woke++, 60000);
  await Bun.sleep(100);
  const before = woke;
  store.emit("page", {});
  await Bun.sleep(300);
  stop();
  expect(woke).toBeGreaterThan(before);
  store.close();
});

test("links are normalised: relative, entity-encoded, tracking parameters dropped, non-pages refused", () => {
  const drop = ["utm_*", "sjid"];
  expect(normalize("/a?utm_source=x&amp;id=2#top", "https://h.test/p/", drop)?.href).toBe("https://h.test/a?id=2");
  expect(normalize("mailto:x@y", "https://h.test/", drop)).toBeNull();
  expect(normalize("/file.pdf", "https://h.test/", drop)).toBeNull();
  expect(normalize("/index.html", "https://h.test/", drop)?.href).toBe("https://h.test/index.html");
});

test("HTTPS records parse in presentation and wire form", () => {
  expect(parseHttpsRecord('1 . alpn="h3,h2" ipv4hint=1.2.3.4')).toEqual({
    priority: 1,
    target: ".",
    alpn: ["h3", "h2"],
  });
  expect(parseHttpsRecord("\\# 19 0001 00 0001 0006 02 6833 02 6832 0004 0004 01020304")).toMatchObject({
    priority: 1,
    target: ".",
    alpn: ["h3", "h2"],
  });
});

test("outliers use the median absolute deviation, as hyperfine does", () => {
  expect(outliers([1, 1.01, 0.99, 1, 1.02, 0.98, 1, 9])).toBe(1);
  expect(outliers([1, 1, 1, 1])).toBe(0);
});

const page = (over: Partial<PageResult>): PageResult => ({
  url: "https://a.test/",
  verdict: "pass",
  final: "https://a.test/",
  status: 200,
  protocol: "h2",
  redirects: 0,
  loaded: true,
  ms: 1,
  title: "",
  lang: null,
  session: null,
  elements: 100,
  custom: 0,
  shadow: 0,
  text: 1000,
  links: 0,
  images: 0,
  scripts: 0,
  frames: 0,
  canvases: 0,
  ttfb: null,
  dcl: null,
  load: null,
  fcp: null,
  lcp: null,
  requests: 0,
  failed: 0,
  http_errors: 0,
  bytes: 0,
  exceptions: 0,
  console_errors: 0,
  shot: null,
  spread: 40,
  ...over,
});

test("verdicts separate the engine's failures from the server's answers", () => {
  const walls = manifest.walls;
  expect(judge(page({}), walls).verdict).toBe("pass");
  expect(judge(page({ status: 403 }), walls).verdict).toBe("refused");
  expect(judge(page({ final: "https://accounts.google.com/v3/signin" }), walls).verdict).toBe("auth");
  expect(judge(page({ elements: 3 }), walls).verdict).toBe("fail");
  expect(judge(page({ spread: 0 }), walls).verdict).toBe("fail");
  expect(judge(page({ error: "net::ERR_X" }), walls).verdict).toBe("fail");
});

test("lite is judged against full by landing place and document size", () => {
  const min = manifest.parity;
  expect(parity(page({ elements: 90, text: 800 }), page({}), min).sufficient).toBe(true);
  expect(parity(page({ elements: 10 }), page({}), min).sufficient).toBe(false);
  expect(parity(page({ final: "https://b.test/" }), page({}), min).why).toContain("lite at b.test");
});

test("an all-grey PNG has no spread, a two-tone one has", async () => {
  const flat = await new Bun.Image(new Uint8Array(4 * 4 * 4).fill(128), {
    raw: { width: 4, height: 4, channels: 4 },
  } as never)
    .png()
    .bytes()
    .catch(() => null);
  if (!flat) return; // this runtime cannot build an image from raw pixels: the check is covered live by `run`
  expect(await spread(flat)).toBe(0);
});

test("hyperfine 1.x and 2.x exports both become a Timing", () => {
  const v1 = {
    results: [
      {
        command: "a",
        mean: 0.5,
        stddev: 0.1,
        median: 0.5,
        user: 0.2,
        system: 0.1,
        min: 0.4,
        max: 0.6,
        times: [0.4, 0.6],
        exit_codes: [0, 0],
      },
    ],
  };
  expect(readHyperfine(v1)[0]).toMatchObject({ mean: 0.5, times: [0.4, 0.6] });
  const v2 = {
    schema_version: 2,
    results: [
      {
        command: "a",
        measurements: [
          { time_wall_clock: { value: 0.4 }, exit_code: 0, memory_usage: { value: 1000 } },
          { time_wall_clock: { value: 0.6 }, exit_code: 1, memory_usage: { value: 3000 } },
        ],
        summary: {
          time_wall_clock: { mean: 0.5, stddev: 0.14, median: 0.5, min: 0.4, max: 0.6 },
          time_user: { mean: 0.2, median: 0.2, min: 0.2, max: 0.2 },
        },
      },
    ],
  };
  expect(readHyperfine(v2)[0]).toMatchObject({
    mean: 0.5,
    stddev: 0.14,
    times: [0.4, 0.6],
    exit_codes: [0, 1],
    memory_usage_byte: [1000, 3000],
    user: 0.2,
    system: 0,
  });
});

test("a bot challenge is refused whatever status the engine saw", () => {
  const walls = manifest.walls;
  for (const title of [
    "Just a moment...",
    "Un instant…",
    "Attention Required! | Cloudflare",
    "SEC.gov | Request Rate Threshold Exceeded",
  ])
    expect(judge(page({ title, status: 200 }), walls)).toMatchObject({ verdict: "refused" });
  expect(judge(page({ title: "Google Design", status: 200 }), walls).verdict).toBe("pass");
});

test("reasons are classified for the router", () => {
  expect(classify("Page.navigate got no answer in 30000 ms")).toBe("hang");
  expect(classify("empty document")).toBe("empty");
  expect(classify("lite pass, full refused")).toBe("verdict-differs");
  expect(classify("lite at a.test, full at b.test")).toBe("redirect-differs");
  expect(classify("0.3 of the elements")).toBe("low-parity");
});

test("lessons count the runs that repeat them and drop the ones an engine caught up on", async () => {
  const store = await open();
  const lesson = { host: "a.test", engine: "lite", kind: "hang", detail: "no answer" };
  const seen = [{ host: "a.test", engine: "lite" }];
  expect(store.learn("r1", [lesson], seen)).toEqual({ added: 1, repeated: 0, dropped: 0 });
  expect(store.learn("r1", [lesson], seen)).toEqual({ added: 0, repeated: 0, dropped: 0 }); // the same run twice counts once
  expect(store.learn("r2", [lesson], seen)).toEqual({ added: 0, repeated: 1, dropped: 0 });
  expect(store.lessons()[0]).toMatchObject({ runs: 2, first_run: "r1", last_run: "r2" });
  expect(store.learn("r3", [], seen)).toEqual({ added: 0, repeated: 0, dropped: 1 });
  expect(store.lessons()).toEqual([]);
  store.learn("r4", [lesson], seen);
  store.learn("r5", [], [{ host: "b.test", engine: "lite" }]); // another host was tested: a.test keeps its lesson
  expect(store.lessons()).toHaveLength(1);
  store.close();
});

test("a database written by the first schema keeps its data and gains the new one", async () => {
  const path = `${tmpdir()}/acceptance-v1-${crypto.randomUUID().slice(0, 8)}.db`;
  const old = new Database(path, { create: true });
  old.exec(
    "CREATE TABLE upstreams (name TEXT NOT NULL, at TEXT NOT NULL, snapshot TEXT NOT NULL, PRIMARY KEY (name, at))",
  );
  old.run("INSERT INTO upstreams VALUES ('servo', '2026-11-01T00:00:00Z', '{\"head\":\"a\"}')");
  old.run("INSERT INTO upstreams VALUES ('servo', '2026-11-02T00:00:00Z', '{\"head\":\"b\"}')");
  old.close();
  const store = await Store.open(path, false);
  expect(store.upstreams()[0].snapshot).toEqual({ head: "b" });
  expect(store.upstreams()[0].history).toHaveLength(2);
  expect(store.saveUpstream("servo", { head: "c" })).toBe(true);
  expect(store.db.query("PRAGMA user_version").get()).toEqual({ user_version: 2 });
  store.close();
  expect((await Store.open(path, false)).upstreams()[0].history).toHaveLength(3); // opening twice is a no-op
  await rmrf(path);
});

test("only failures that say nothing about the engine are tried again", () => {
  expect(isTransient("net::ERR_NETWORK_CHANGED")).toBe(true);
  expect(isTransient("Page.navigate got no answer in 30000 ms")).toBe(true);
  expect(isTransient("empty document")).toBe(false);
  expect(isTransient("blank paint")).toBe(false);
  expect(isTransient(undefined)).toBe(false);
});

test("the private config is laid over the shared one: tables merge, values and arrays are replaced", () => {
  const shared = { serve: { port: 4680 }, track: { chromium: { kind: "engine", pin_deps: "v" } }, seeds: ["a"] };
  const mine = { serve: { host: "h.test" }, track: { chromium: { checkout: { ssh: "x" } } }, seeds: ["b"] };
  expect(merge<unknown>(shared, mine)).toEqual({
    serve: { port: 4680, host: "h.test" },
    track: { chromium: { kind: "engine", pin_deps: "v", checkout: { ssh: "x" } } },
    seeds: ["b"],
  });
  expect(merge<unknown>(shared, {})).toEqual(shared);
});

test("a note is kept once and the latest probe of each host is what telemetry returns", async () => {
  const store = await open();
  store.addNote("gotcha", "a thing", "agent");
  store.addNote("gotcha", "a thing", "agent");
  store.addNote("fact", "another", "agent");
  expect(store.notes().map((n) => n.text)).toEqual(["another", "a thing"]);
  store.saveTelemetry("build", { cores: 4 });
  await Bun.sleep(5);
  store.saveTelemetry("build", { cores: 8 });
  store.saveTelemetry("local", { cores: 2 });
  expect(store.telemetry().map((t) => [t.host, t.data.cores])).toEqual([
    ["build", 8],
    ["local", 2],
  ]);
  store.close();
});
