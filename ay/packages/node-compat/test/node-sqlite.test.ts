import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import nodeSqlite, { DatabaseSync } from "../src/node-sqlite.ts";

const workDir = mkdtempSync(join(tmpdir(), "ay-node-sqlite-"));
afterAll(() => rmSync(workDir, { recursive: true, force: true }));

describe("node-sqlite", () => {
  test("open :memory: + exec + prepare", () => {
    const db = new DatabaseSync(":memory:");
    db.exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
    db.prepare("INSERT INTO t (name) VALUES (?)").run("alice");
    const row = db.prepare("SELECT name FROM t WHERE id = ?").get(1) as { name: string };
    expect(row.name).toBe("alice");
    db.close();
  });

  test("foreign_keys on by default", () => {
    const db = new DatabaseSync();
    const r = db.prepare("PRAGMA foreign_keys").get() as { foreign_keys: number };
    expect(r.foreign_keys).toBe(1);
    db.close();
  });

  test("isOpen is false after close()", () => {
    const db = new DatabaseSync(":memory:");
    expect(db.isOpen).toBe(true);
    db.close();
    expect(db.isOpen).toBe(false);
  });

  test("enableForeignKeyConstraints: false leaves foreign keys off", () => {
    const db = new DatabaseSync(":memory:", { enableForeignKeyConstraints: false });
    const r = db.prepare("PRAGMA foreign_keys").get() as { foreign_keys: number };
    expect(r.foreign_keys).toBe(0);
    db.close();
  });

  test("all() and iterate() return every row", () => {
    const db = new DatabaseSync();
    db.exec("CREATE TABLE n (v INTEGER); INSERT INTO n VALUES (1), (2), (3)");
    const stmt = db.prepare("SELECT v FROM n ORDER BY v");
    expect(stmt.all()).toEqual([{ v: 1 }, { v: 2 }, { v: 3 }]);
    expect([...stmt.iterate()]).toEqual([{ v: 1 }, { v: 2 }, { v: 3 }]);
    db.close();
  });

  test("readOnly opens an existing file and rejects writes", () => {
    const path = join(workDir, "ro.db");
    const rw = new DatabaseSync(path);
    rw.exec("CREATE TABLE t (id INTEGER)");
    rw.close();
    const ro = new DatabaseSync(path, { readOnly: true });
    expect(ro.location).toBe(path);
    expect(() => ro.exec("INSERT INTO t VALUES (1)")).toThrow();
    ro.close();
  });

  test("the default export exposes DatabaseSync", () => {
    expect(nodeSqlite.DatabaseSync).toBe(DatabaseSync);
  });
});
