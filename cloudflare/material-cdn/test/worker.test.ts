import { describe, expect, test } from "bun:test";
import { built, context, fakeEnv } from "./env.ts";
import { readSfnt } from "./sfnt.ts";
import { instantiate, loadInto, subsetAt, Flag } from "../src/hb.ts";
import { iconFont, resolveAxes, textFont } from "../src/fonts.ts";
import { loadMeta, loadSymbols } from "../src/meta.ts";
import { parse } from "../../../tools/material-cdn/grammar.ts";
import worker from "../worker.ts";

const suite = built ? describe : describe.skip;
const get = (path: string, init?: RequestInit) => {
  const { env } = shared;
  return worker.fetch(new Request(`https://material.aphrody.com${path}`, init), env, context());
};
const shared = fakeEnv();

suite("generated outputs against the built tree", () => {
  test("icon sprite: outline and filled symbols with the site generator's ids", async () => {
    const res = await get("/v1/icons.svg?style=rounded&icons=search,home&fill=0..1&ids=of");
    expect(res.status).toBe(200);
    const svg = await res.text();
    expect([...svg.matchAll(/<symbol id="([^"]+)"/g)].map((m) => m[1])).toEqual(["o-home", "f-home", "o-search", "f-search"]);
    expect(svg).toMatch(/^<svg xmlns="http:\/\/www.w3.org\/2000\/svg"><symbol id="o-home" viewBox="0 -960 960 960"><path d="M/);
    expect(res.headers.get("ETag")).toMatch(/^"[0-9a-f]{20}"$/);
    expect(res.headers.get("Content-Location")).toMatch(/^\/v1\/g\/[0-9a-f]{20}\.svg$/);
  });

  test("equal needs in any order share one canonical hash; the second call is served from the store", async () => {
    const a = await get("/v1/icons.json?icons=add,home&style=outlined&wght=500");
    const b = await get("/v1/icons.json?wght=500&style=outlined&icons=home,add,add");
    expect(a.headers.get("ETag")).toBe(b.headers.get("ETag"));
    const json = (await b.json()) as { wght: number; icons: Record<string, Record<string, string>> };
    expect(json.wght).toBe(500);
    expect(Object.keys(json.icons)).toEqual(["add", "home"]);
    expect(json.icons.home!["0"]).toMatch(/^M/);
    expect(["r2", "edge-cache"]).toContain(b.headers.get("X-Material-Source")!);
  });

  test("a pinned revision is immutable; an unpinned one is cached for a day with an ETag", async () => {
    const meta = await loadMeta(shared.env);
    const unpinned = await get("/v1/icons.json?icons=home");
    expect(unpinned.headers.get("Cache-Control")).toContain("max-age=86400");
    const pinned = await get(`/v1/icons.json?icons=home&v=${meta.rev}`);
    expect(pinned.headers.get("Cache-Control")).toBe("public, max-age=31536000, immutable");
    const revalidated = await get("/v1/icons.json?icons=home", { headers: { "If-None-Match": unpinned.headers.get("ETag")! } });
    expect(revalidated.status).toBe(304);
    const stale = await get("/v1/icons.json?icons=home&v=aaaaaaaaaaaa");
    expect(stale.status).toBe(404);
  });

  test("unknown icons and bad parameters are 400 with the offenders listed", async () => {
    const res = await get("/v1/icons.svg?icons=home,definitely_not_an_icon");
    expect(res.status).toBe(400);
    expect(((await res.json()) as { detail: { unknown: string[] } }).detail.unknown).toEqual(["definitely_not_an_icon"]);
    expect((await get("/v1/icons.svg?icons=home&wght=450")).status).toBe(400);
    expect((await get("/v1/icons.svg?icons=home&opsz=48")).status).toBe(400);
    expect((await get("/v1/icons.woff2?icons=home&bogusaxis=3")).status).toBe(400);
  });

  test("POST takes a list too long for a URL", async () => {
    const res = await get("/v1/icons.svg?style=sharp&fill=1", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ icons: ["home", "search", "settings"] }) });
    expect(res.status).toBe(200);
    expect((await res.text()).match(/<symbol /g)).toHaveLength(3);
    const text = await get("/v1/icons.json?style=sharp", { method: "POST", body: "home\nsearch settings" });
    expect(Object.keys(((await text.json()) as { icons: object }).icons)).toEqual(["home", "search", "settings"]);
  });

  test("icon font: the glyph count matches the request exactly, ligatures included", async () => {
    const meta = await loadMeta(shared.env);
    const symbols = await loadSymbols(shared.env, meta, "outlined");
    const icons = ["home", "search", "settings", "menu", "close"];
    const letters = new Set(icons.join("")).size;
    const family = meta.families["material-symbols-outlined"]!;
    for (const [fill, wanted] of [
      [[0, 0], icons.length],
      [[0, 1], icons.length + icons.filter((n) => symbols.fill[symbols.index.get(n)!]).length],
    ] as const) {
      const need = parse("icons-font", new URLSearchParams({ icons: icons.join(","), fill: fill[0] === fill[1] ? "0" : "0..1" })).need;
      const axes = resolveAxes(family, need);
      const object = await shared.env.MATERIAL.get(`v1/${family.src}`);
      const x = instantiate();
      const { pointer, length } = await loadInto(x, object!.body, object!.size);
      const ttf = subsetAt(x, pointer, length, {
        unicodes: [...icons.map((n) => symbols.cp[symbols.index.get(n)!]!), ...new Set([...icons.join("")].map((c) => c.charCodeAt(0)))],
        glyphs: fill[1] > 0 ? icons.map((n) => symbols.fill[symbols.index.get(n)!]!).filter(Boolean) : [],
        flags: Flag.NoHinting | Flag.NoLayoutClosure | Flag.OptimizeIupDeltas,
        pin: axes.pin,
        pinDefault: axes.pinDefault,
        limit: axes.limit,
      });
      const facts = readSfnt(ttf);
      expect(facts.glyphs).toBe(1 + letters + wanted);
      expect(facts.ligatures).toBe(icons.length);
      expect(facts.codepoints).toHaveLength(letters + icons.length);
      expect(facts.axes).toEqual(fill[1] > 0 ? ["FILL"] : []);
    }
  });

  test("icons.woff2 is a woff2 and is smaller than a thousandth of the full font", async () => {
    const res = await get("/v1/icons.woff2?style=outlined&icons=home,search,settings,menu,close&fill=0..1&wght=400");
    expect(res.status).toBe(200);
    const bytes = new Uint8Array(await res.arrayBuffer());
    expect(String.fromCharCode(...bytes.slice(0, 4))).toBe("wOF2");
    expect(bytes.length).toBeLessThan(10_000);
  });

  test("text font: only the requested characters, axes instanced", async () => {
    const meta = await loadMeta(shared.env);
    const need = parse("font", new URLSearchParams({ text: "Aphrody", wght: "400..700", wdth: "100" }), "google-sans-flex").need;
    const woff2 = await textFont(shared.env, meta, need);
    expect(String.fromCharCode(...woff2.slice(0, 4))).toBe("wOF2");
    const family = meta.families["google-sans-flex"]!;
    const axes = resolveAxes(family, need);
    expect(axes.pin.wdth).toBe(100);
    expect(axes.limit.wght).toEqual([400, 700]);
    expect(axes.pinDefault.sort()).toEqual(["ROND", "opsz", "slnt"]);
    expect((await get("/v1/fonts/google-sans-flex.woff2")).status).toBe(400);
    expect((await get("/v1/fonts/google-sans-flex.woff2?text=a&wght=1000..2000")).status).toBe(400);
    expect((await get("/v1/fonts/no-such-font.woff2?text=a")).status).toBe(400);
  });

  test("font css has one immutable woff2 url and unicode-range per subset", async () => {
    const res = await get("/v1/fonts/google-sans-code.css?subsets=latin,greek&wght=400..700");
    expect(res.status).toBe(200);
    const css = await res.text();
    const meta = await loadMeta(shared.env);
    expect(css.match(/@font-face/g)).toHaveLength(2);
    expect(css).toContain(`google-sans-code.woff2?subsets=greek&wght=400..700&v=${meta.rev}`);
    expect(css).toContain("font-weight:400 700");
    expect(css).toMatch(/unicode-range:U\+0370-0377/);
  });

  test("icons.css names every icon and keeps FILL as a class when it is variable", async () => {
    const css = await (await get("/v1/icons.css?style=outlined&icons=home,arrow_back&fill=0..1&class=mi")).text();
    expect(css).toContain(".mi-arrow-back::before{content:\"\\e5c4\"}");
    expect(css).toContain(".mi-fill{font-variation-settings:'FILL' 1}");
    expect(css).toContain("font-display:block");
  });

  test("single icon and the aphrody set", async () => {
    const svg = await (await get("/v1/icon.svg?icons=home&style=sharp&fill=1&size=24&color=%23fff")).text();
    expect(svg).toMatch(/^<svg xmlns="http:\/\/www.w3.org\/2000\/svg" viewBox="0 -960 960 960" width="24" height="24" fill="#fff"><path d="M/);
    const own = await get("/v1/icons.svg?set=aphrody&icons=aphrody_character,file_icons/rust");
    expect(own.status).toBe(200);
    expect((await own.text()).match(/<symbol /g)).toHaveLength(2);
  });
});
