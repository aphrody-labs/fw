import { describe, expect, test } from "bun:test";
import fixtures from "../../../tools/material-cdn/fixtures/urls.json";
import { material } from "../../../tools/material-cdn/client.ts";
import { canonicalQuery, formatUnicodes, GrammarError, identity, outputOf, parse, parseRange, parseUnicodes } from "../../../tools/material-cdn/grammar.ts";

const q = (s: string) => new URLSearchParams(s);

describe("parse and canonical form", () => {
  test("equal needs have one canonical query whatever the order", () => {
    const a = parse("sprite", q("icons=search,home,home&style=rounded&wght=400&fill=0..1&ids=of"));
    const b = parse("sprite", q("ids=of&fill=0..1&style=rounded&icons=home,search&WGHT=400"));
    expect(a.canonical).toBe(b.canonical);
    expect(a.canonical).toBe("fill=0..1&icons=home,search&ids=of&style=rounded&wght=400");
  });

  test("text, unicodes and the + of U+ merge into sorted unique codepoints", () => {
    const { need, canonical } = parse("font", q("text=Aphrody&unicodes=U%2B0041-0043"), "google-sans-flex");
    expect(need.unicodes).toEqual([0x41, 0x42, 0x43, 0x64, 0x68, 0x6f, 0x70, 0x72, 0x79]);
    expect(canonical).toBe("unicodes=41-43,64,68,6F-70,72,79");
    // a literal + in a query string decodes to a space: the parser accepts that form too
    expect(parseUnicodes("U 20-22")).toEqual([0x20, 0x21, 0x22]);
  });

  test("axes: pins, ranges, negative minimums", () => {
    expect(parseRange("400", "wght")).toEqual([400, 400]);
    expect(parseRange("100..700", "wght")).toEqual([100, 700]);
    expect(parseRange("-50..200", "grad")).toEqual([-50, 200]);
    expect(parseRange("-50-200", "grad")).toEqual([-50, 200]);
    expect(parseRange("0.5", "fill")).toEqual([0.5, 0.5]);
    expect(() => parseRange("700..100", "wght")).toThrow(GrammarError);
    expect(() => parseRange("bold", "wght")).toThrow(GrammarError);
  });

  test("rejects what is not a name, a range or a parameter", () => {
    expect(() => parse("sprite", q("icons=Home"))).toThrow(/valid icon name/);
    expect(() => parse("sprite", q("icons=../etc"))).toThrow(GrammarError);
    expect(() => parse("sprite", q("style=bold"))).toThrow(/style/);
    expect(() => parse("sprite", q("bogus=1"))).toThrow(/unknown parameter/);
    expect(() => parse("font", q("color=%3Cscript%3E"), "x")).toThrow(/color/);
    expect(() => parse("icons-css", q("class=1bad"))).toThrow(/class/);
    expect(() => parse("sprite", q(`icons=${Array.from({ length: 1001 }, (_, i) => `n${i}`).join(",")}`))).toThrow(/at most 1000/);
  });

  test("aphrody names may be folders", () => {
    expect(parse("sprite", q("set=aphrody&icons=file_icons/rust,ai_claude")).need.icons).toEqual(["ai_claude", "file_icons/rust"]);
    expect(() => parse("sprite", q("icons=file_icons/rust"))).toThrow(GrammarError);
  });

  test("identity drops parameters that do not change the bytes", () => {
    const font = (extra: string) => canonicalQuery(identity(parse("icons-font", q(`icons=home&${extra}`)).need));
    expect(font("class=a&family=B&display=swap")).toBe(font(""));
    const css = (extra: string) => canonicalQuery(identity(parse("icons-css", q(`icons=home&${extra}`)).need));
    expect(css("class=a")).not.toBe(css("class=b"));
  });

  test("paths map back to outputs", () => {
    expect(outputOf("icons.svg")).toEqual({ output: "sprite" });
    expect(outputOf("fonts/roboto-flex.css")).toEqual({ output: "font-css", family: "roboto-flex" });
    expect(outputOf("fonts/google-sans-flex/latin.abcd1234.woff2")).toBeUndefined();
    expect(outputOf("index.json")).toBeUndefined();
  });

  test("unicode formatting", () => {
    expect(formatUnicodes([0x20, 0x21, 0x22, 0x41])).toBe("U+20-22,U+41");
    expect(formatUnicodes([0x1f600], "")).toBe("1F600");
  });
});

describe("typed client", () => {
  const cdn = material("https://material.aphrody.com/v1/");
  for (const fixture of fixtures) {
    test(`fixture ${fixture.name}`, () => {
      expect((cdn as unknown as Record<string, (n: unknown) => string>)[fixture.method]!(fixture.needs)).toBe(fixture.url);
    });
  }

  test("every client URL parses back to the same canonical query", () => {
    for (const fixture of fixtures) {
      const url = new URL(fixture.url);
      const route = outputOf(url.pathname.replace("/v1/", ""))!;
      const params = new URLSearchParams(url.search);
      params.delete("v");
      expect(parse(route.output, params, route.family).canonical).toBe(url.search.slice(1).replace(/&?v=[0-9a-f]+/, ""));
    }
  });
});
