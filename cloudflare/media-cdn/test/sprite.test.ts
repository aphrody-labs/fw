import { describe, expect, test } from "bun:test";
import { symbolOf } from "../../../tools/media-cdn/sprite.ts";

describe("sprite symbols", () => {
  test("an SVG becomes a symbol with its viewBox and inner markup", () => {
    const svg = '<?xml version="1.0"?><!-- c --><svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 16 16"><path d="M0 0h16v16z"/></svg>\n';
    expect(symbolOf("home", svg)).toBe('<symbol id="home" viewBox="0 0 16 16"><path d="M0 0h16v16z"/></symbol>');
  });

  test("without a viewBox the width and height give the box", () => {
    expect(symbolOf("a", '<svg width="20px" height="12"><rect/></svg>')).toBe('<symbol id="a" viewBox="0 0 20 12"><rect/></symbol>');
  });

  test("no box at all, scripts, handlers and external references are refused", () => {
    expect(() => symbolOf("a", "<svg><rect/></svg>")).toThrow("no viewBox");
    expect(() => symbolOf("a", '<svg viewBox="0 0 1 1"><script>1</script></svg>')).toThrow("not allowed");
    expect(() => symbolOf("a", '<svg viewBox="0 0 1 1"><rect onload="x()"/></svg>')).toThrow("not allowed");
    expect(() => symbolOf("a", '<svg viewBox="0 0 1 1"><image href="https://x.example/a.png"/></svg>')).toThrow("not allowed");
    expect(() => symbolOf("a", "not svg")).toThrow("not an SVG");
  });

  test("an internal reference is allowed", () => {
    expect(symbolOf("a", '<svg viewBox="0 0 1 1"><use href="#b"/></svg>')).toContain('href="#b"');
  });
});
