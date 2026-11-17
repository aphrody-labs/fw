import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { audit, families, lockVersions, normalizeSql } from "./dedup.ts";

const dirs: string[] = [];
const repo = (files: Record<string, string>) => {
  const dir = mkdtempSync(join(tmpdir(), "dedup-"));
  dirs.push(dir);
  for (const [name, text] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, name)), { recursive: true });
    writeFileSync(join(dir, name), text);
  }
  return dir;
};
afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

test("SQL is normalized so the same statement with other literals and placeholders matches", () => {
  expect(normalizeSql("SELECT  id FROM Pages WHERE url = 'x' AND n > 3")).toBe(normalizeSql("select id from pages where url = 'y' and n > 7"));
  expect(normalizeSql("select a from t where id = $1")).toBe(normalizeSql("select a from t where id = :id"));
});

test("lockfile versions are read per crate", () => {
  const v = lockVersions('[[package]]\nname = "serde"\nversion = "1.0.1"\n\n[[package]]\nname = "serde"\nversion = "1.0.2"\n\n[[package]]\nname = "once"\nversion = "1.0.0"\n');
  expect([...v.get("serde")!]).toEqual(["1.0.1", "1.0.2"]);
});

test("families group names across repositories after the organization prefix and suffix are removed", () => {
  const f = families([{ repo: "a", name: "aphrody-web-crawl" }, { repo: "b", name: "yolo-web-crawl-core" }, { repo: "a", name: "unique-thing" }]);
  expect(f.map((x) => x.key)).toEqual(["web-crawl"]);
  expect(f[0]!.members).toEqual(["a:aphrody-web-crawl", "b:yolo-web-crawl-core"]);
});

test("the audit finds duplicated SQL, database libraries, crawler code, repeated lock versions and name families", () => {
  const stmt = 'db.query("SELECT url, hash FROM pages WHERE etag = ?")';
  const a = repo({
    "Cargo.toml": '[package]\nname = "aphrody-search"\n',
    "Cargo.lock": '[[package]]\nname = "regex"\nversion = "1.1.0"\n\n[[package]]\nname = "regex"\nversion = "1.2.0"\n',
    "src/store.ts": `import { Database } from "bun:sqlite";\n${stmt}`,
    "tools/crawl.ts": "export {}",
    "vendor/skip.ts": stmt,
  });
  const b = repo({ "package.json": '{"name":"@x/search"}', "src/db.rs": `use rusqlite::Connection;\nlet s = "SELECT url, hash FROM pages WHERE etag = ?1";`, "mirror/run.py": "pass" });
  const r = audit([{ name: "a", dir: a }, { name: "b", dir: b }]);
  expect(r.sql).toHaveLength(1);
  expect(r.sql[0]!.members.map((m) => m.split(":")[0]).sort()).toEqual(["a", "b"]);
  expect(Object.keys(r.dbLibraries).sort()).toEqual(["bun:sqlite", "rusqlite"]);
  expect(Object.keys(r.crawlers).sort()).toEqual(["a", "b"]);
  expect(r.lockDuplicates).toEqual([{ repo: "a", name: "regex", versions: ["1.1.0", "1.2.0"] }]);
  expect(r.families.map((f) => f.key)).toEqual(["search"]);
});
