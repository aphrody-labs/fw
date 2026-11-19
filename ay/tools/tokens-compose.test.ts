import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { COMPOSE_ROLES, composeColor, kotlinPackage, render, type Tokens } from "./tokens-compose.ts";

const tokens = JSON.parse(readFileSync(join(import.meta.dir, "..", "brand", "tokens", "m3-tokens.json"), "utf8")) as Tokens;

describe("tokens-compose", () => {
  test("colours and package names", () => {
    expect(composeColor("#676013")).toBe("Color(0xFF676013)");
    expect(() => composeColor("red")).toThrow();
    expect(kotlinPackage("aphrody.com")).toBe("com.aphrody");
  });

  test("the committed tokens cover every Compose Material 3 role in both schemes", () => {
    for (const s of [tokens.light, tokens.dark]) for (const r of COMPOSE_ROLES) expect(s[r], r).toMatch(/^#[0-9a-f]{6}$/i);
    const kt = render(tokens, "com.aphrody");
    expect(kt).toContain("package com.aphrody.theme");
    expect(kt).toContain(`primary = ${composeColor(tokens.light.primary as string)},`);
    expect(kt.match(/ = Color\(0xFF/g)?.length).toBe(COMPOSE_ROLES.length * 2);
    expect(kt).toContain("fun AphrodyTheme(");
    expect(kt).toContain("typography: Typography? = null,");
    expect(kt).toContain("motionScheme = MotionScheme.expressive(),");
  });

  test("a missing role is an error, not a silent default", () => {
    const { primary: _drop, ...light } = tokens.light;
    expect(() => render({ ...tokens, light }, "com.aphrody")).toThrow(/primary/);
  });
});
