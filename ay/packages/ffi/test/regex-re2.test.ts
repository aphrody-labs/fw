import { test, expect, describe } from "bun:test";
import { Re2, re2 } from "../src/regex-re2.ts";

describe("regex-re2", () => {

  // 1. Partial match (test)
  test("new Re2('fo+').test('foobar') -> true", () => {
    using r = new Re2("fo+");
    expect(r.test("foobar")).toBe(true);
    expect(r.test("bar")).toBe(false);
  });

  // 2. fullMatch -- positive
  test("fullMatch: 'foo' matches /fo+/", () => {
    using r = new Re2("fo+");
    expect(r.fullMatch("foo")).toBe(true);
  });

  // 3. fullMatch -- negative (prefix only)
  test("fullMatch: 'foobar' does NOT full-match /fo+/", () => {
    using r = new Re2("fo+");
    expect(r.fullMatch("foobar")).toBe(false);
  });

  // 4. Invalid pattern throws at construction
  test("invalid pattern throws at construction", () => {
    expect(() => new Re2("(unclosed")).toThrow(/RE2 compile error/);
  });

  // 5. Capture group: exec returns groups array
  test("exec('abc123') with /(\\d+)/ -> ['123']", () => {
    using r = new Re2("(\\d+)");
    const result = r.exec("abc123");
    expect(result).not.toBeNull();
    expect(result).toEqual(["123"]);
  });

  // 6. Re2 vs JS RegExp -- same result on a common pattern
  test("Re2 matches the same strings as JS RegExp for /[a-z]+@[a-z]+\\.[a-z]+/", () => {
    const pattern = "[a-z]+@[a-z]+\\.[a-z]+";
    const jsRe = new RegExp(pattern);
    using re2Inst = new Re2(pattern);
    const testCases = [
      "user@example.com",
      "INVALID",
      "hello@world.org",
      "no-at-sign",
      "a@b.c",
    ];
    for (const tc of testCases) {
      expect(re2Inst.test(tc)).toBe(jsRe.test(tc));
    }
  });

  // 7. Performance: 1M iterations on a simple pattern < 2s
  test("performance: 1M partial matches < 2000ms", () => {
    using r = new Re2("\\d+");
    const start = performance.now();
    for (let i = 0; i < 1_000_000; i++) {
      r.test("hello world 42");
    }
    const elapsed = performance.now() - start;
    expect(elapsed).toBeLessThan(2000);
    console.log(`  1M partial matches: ${elapsed.toFixed(1)}ms`);
  });

  // 8. [Symbol.dispose] / close does not crash
  test("[Symbol.dispose] and close() are idempotent and do not crash", () => {
    const r = new Re2("abc");
    expect(() => {
      r.close();
      r.close();   // double-close must be a no-op
    }).not.toThrow();

    expect(() => {
      using scoped = new Re2("xyz");
      scoped.test("xyz");
      // Disposed automatically at end of block
    }).not.toThrow();
  });

  // Bonus: multiple capturing groups
  test("exec with multiple groups: /(\\w+)\\s+(\\w+)/ on 'hello world'", () => {
    using r = new Re2("(\\w+)\\s+(\\w+)");
    const result = r.exec("hello world");
    expect(result).toEqual(["hello", "world"]);
  });

  // Bonus: case-insensitive flag
  test("case-insensitive flag: re2('foo', 'i').test('FOO') -> true", () => {
    using r = re2("foo", "i");
    expect(r.test("FOO")).toBe(true);
    expect(r.test("bar")).toBe(false);
  });

  // Bonus: no match returns null from exec
  test("exec returns null when pattern does not match", () => {
    using r = new Re2("(\\d+)");
    expect(r.exec("no digits here")).toBeNull();
  });

  test("exec on a pattern without groups returns [] on a match and null otherwise", () => {
    using r = new Re2("fo+");
    expect(r.exec("foobar")).toEqual([]);
    expect(r.exec("bar")).toBeNull();
  });

  test("exec reports at most 32 groups without reading past the buffer", () => {
    using r = new Re2("(a)".repeat(40));
    const result = r.exec("a".repeat(40));
    expect(result).toEqual(Array.from({ length: 32 }, () => "a"));
  });

  test("source returns the pattern including the inline flags", () => {
    using r = new Re2("foo", "i");
    expect(r.source).toBe("(?i)foo");
  });

  test("use after dispose throws", () => {
    const r = new Re2("abc");
    r.close();
    expect(() => r.test("abc")).toThrow("Re2 instance has been disposed");
  });

  test("exec decodes non-ASCII groups", () => {
    using r = new Re2("(\\p{L}+)");
    expect(r.exec("12 café 34")).toEqual(["café"]);
  });
});
