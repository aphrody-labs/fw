import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { privatePath } from "./local.ts";
import { toml } from "./org-inventory.ts";

test("the TOML writer round-trips tables, arrays of tables, scalars and quoted keys", () => {
  const value = {
    version: 1,
    list: ["a", "b"],
    zone: [{ name: "x.test", types: { A: 2, "odd key": 1 } }, { name: "y.test", types: { MX: 1 } }],
    nested: { ok: true, none: null },
  };
  const parsed = Bun.TOML.parse(toml(value)) as any;
  expect(parsed.version).toBe(1);
  expect(parsed.list).toEqual(["a", "b"]);
  expect(parsed.zone.map((z: any) => z.name)).toEqual(["x.test", "y.test"]);
  expect(parsed.zone[0].types).toEqual({ A: 2, "odd key": 1 });
  expect(parsed.nested).toEqual({ ok: true });
});

test.skipIf(!existsSync(privatePath("org-inventory.toml")))("the private inventory parses and records the evidence commands", async () => {
  const parsed = Bun.TOML.parse(await Bun.file(privatePath("org-inventory.toml")).text()) as any;
  expect(parsed.commands.github).toContain("gh repo list");
  expect(Object.keys(parsed.github).length).toBeGreaterThanOrEqual(3);
  expect(JSON.stringify(parsed)).not.toMatch(/[0-9a-f]{32}/);
});
