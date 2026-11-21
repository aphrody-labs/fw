import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { privatePath } from "./local.ts";
import { loadClaims, type Result } from "./claims.ts";
import { build, health, render, type Item } from "./plan.ts";

const r = (id: string, status: Result["status"]): Result => ({ id, text: id, status, observed: "o", checked: "2026-11-17" });

test("health is verified only when every linked claim is proved", () => {
  const results = new Map([["a", r("a", "proved")], ["b", r("b", "failed")], ["c", r("c", "error")]]);
  const item = (claims?: string[]): Item => ({ id: "i", title: "I", status: "running", claims });
  expect(health(item(["a"]), results)).toBe("verified");
  expect(health(item(["a", "b"]), results)).toBe("attention");
  expect(health(item(["c"]), results)).toBe("attention");
  expect(health(item(["missing"]), results)).toBe("attention");
  expect(health(item(), results)).toBe("unchecked");
});

test("the page orders blocked work first and lists claims that do not hold", () => {
  const work: Item[] = [{ id: "1", title: "Done thing", status: "done" }, { id: "2", title: "Blocked thing", status: "blocked", next: "owner" }];
  const page = render(work, [r("x", "failed")], "| [0001](a.md) | One | accepted | now |\n", [{ product: "os", status: "done" }, { product: "os", status: "todo" }], undefined);
  expect(page.indexOf("Blocked thing")).toBeLessThan(page.indexOf("Done thing"));
  expect(page).toContain("| x (`x`) | failed | o |");
  expect(page).toContain("| [0001](a.md) | One | accepted |");
  expect(page).toContain("| os | 1 | 1 |");
});

test.skipIf(!existsSync(privatePath("plan/work.toml")))("every claim an item names exists, and the plan page is what the private sources generate", async () => {
  const ids = new Set((await loadClaims()).map((c) => c.id));
  const work = (Bun.TOML.parse(await Bun.file(privatePath("plan/work.toml")).text()) as { item: Item[] }).item;
  for (const i of work) for (const c of i.claims ?? []) expect(ids.has(c), `${i.id}: ${c}`).toBe(true);
  expect(new Set(work.map((i) => i.id)).size).toBe(work.length);
  expect(await Bun.file(privatePath("plan.md")).text()).toBe(await build());
});
