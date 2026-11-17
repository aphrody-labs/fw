import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { merge, privateDir, privatePath, readAy } from "./local.ts";

const dirs: string[] = [];
const tree = (files: Record<string, string>) => {
  const dir = mkdtempSync(join(tmpdir(), "local-"));
  dirs.push(dir);
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
  return dir;
};
afterEach(() => {
  delete Bun.env.AY_PRIVATE_DIR;
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

test("tables merge key by key and arrays are replaced", () => {
  expect(merge({ a: { x: 1, y: 2 }, l: [1, 2], k: 1 }, { a: { y: 3, z: 4 }, l: [9] })).toEqual({ a: { x: 1, y: 3, z: 4 }, l: [9], k: 1 });
});

test("ay.local.toml extends ay.toml and is optional", () => {
  const plain = tree({ "ay.toml": '[hosts]\nbuild = "a"\n[repos]\nx = { slug = "o/x" }\n' });
  expect(readAy(plain)).toEqual({ hosts: { build: "a" }, repos: { x: { slug: "o/x" } } });
  const both = tree({ "ay.toml": '[hosts]\nbuild = "a"\n[repos]\nx = { slug = "o/x" }\n', "ay.local.toml": '[hosts]\ntest = "b"\n[repos.x]\ncheckout = "../x"\n' });
  expect(readAy(both)).toEqual({ hosts: { build: "a", test: "b" }, repos: { x: { slug: "o/x", checkout: "../x" } } });
});

test("the private directory defaults to .private and can be moved", () => {
  const root = tree({});
  expect(privateDir(root)).toBe(join(root, ".private"));
  Bun.env.AY_PRIVATE_DIR = "elsewhere";
  expect(privateDir(root)).toBe(join(root, "elsewhere"));
  mkdirSync(join(root, "elsewhere"), { recursive: true });
  expect(privatePath("a/b.txt", root, true)).toBe(join(root, "elsewhere", "a", "b.txt"));
});
